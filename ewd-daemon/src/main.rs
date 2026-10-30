use anyhow::Result;
use ewd_core::events::DaemonEvent;
use ewd_core::manager::{ManagerCommand, ManagerError, WebViewManager};
use ewd_core::tab::Tab;
use ewd_daemon::ipc::IpcServer;
use gtk4::glib::prelude::*;
use gtk4::prelude::GtkWindowExt;
use nix::sys::signal;
use nix::unistd::Pid;
use std::env;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;
use webkit6::prelude::*;

fn is_process_alive(pid: i32) -> bool {
	if pid <= 0 {
		return false;
	}
	match signal::kill(Pid::from_raw(pid), None) {
		Ok(_) => true,
		Err(nix::errno::Errno::ESRCH) => false,
		Err(_) => true,
	}
}

fn main() -> Result<()> {
	tracing_subscriber::fmt::init();
	info!("Starting EWD daemon");

	let args: Vec<String> = std::env::args().collect();
	let proxy_uri = args
		.iter()
		.position(|arg| arg == "--proxy")
		.and_then(|i| args.get(i + 1).cloned());
	if let Some(ref proxy) = proxy_uri {
		info!("Using proxy: {}", proxy);
	}

	gtk4::init().expect("Failed to initialize GTK4");
	info!("GTK4 initialized successfully");

	let (manager, mut gtk_rx) = WebViewManager::new(proxy_uri);
	let manager = Arc::new(manager);
	let main_loop = gtk4::glib::MainLoop::new(None, false);
	let main_loop_clone = main_loop.clone();
	let manager_tokio = Arc::clone(&manager);
	let manager_ipc = Arc::clone(&manager);

	let socket_path = env::var("EWD_SOCKET_PATH").unwrap_or_else(|_| {
		env::var("XDG_RUNTIME_DIR")
			.map(|dir| format!("{dir}/webkit-bridge.sock"))
			.expect("XDG_RUNTIME_DIR is not set. Please set it or use EWD_SOCKET_PATH.")
	});
	let socket_path_clone = socket_path.clone();

	std::thread::spawn(move || {
		let rt = tokio::runtime::Builder::new_multi_thread()
			.enable_all()
			.build()
			.expect("Failed to build Tokio runtime");
		rt.block_on(async move {
			if Path::new(&socket_path_clone).exists() {
				warn!("Removing stale socket file at {}", socket_path_clone);
				let _ = std::fs::remove_file(&socket_path_clone);
			}
			let server = IpcServer::new(&socket_path_clone, manager_ipc);
			let server_handle = tokio::spawn(async move { if let Err(e) = server.start().await { warn!("Server error: {}", e); } });

			let manager_for_cleanup = Arc::clone(&manager_tokio);
			tokio::spawn(async move {
				let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
				loop { interval.tick().await; manager_for_cleanup.cleanup_orphans().await; }
			});

			let mut shutdown_rx = manager_tokio.subscribe();
			tokio::select! {
				_ = tokio::signal::ctrl_c() => { info!("Ctrl+C received in Tokio thread"); }
				_ = async { while let Ok(event) = shutdown_rx.recv().await { if matches!(event, DaemonEvent::Shutdown) { break; } } } => {
					info!("Shutdown event received via IPC in Tokio thread");
				}
			}
			info!("Shutting down EWD daemon (Tokio cleanup)");
			server_handle.abort();
			if Path::new(&socket_path_clone).exists() {
				let _ = std::fs::remove_file(&socket_path_clone);
				info!("Removed socket file {}", socket_path_clone);
			}
			main_loop_clone.quit();
		});
	});

	let main_loop_for_shutdown = main_loop.clone();
	gtk4::glib::MainContext::default().spawn_local(async move {
		let tabs = std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::<Uuid, Tab>::new()));
		let mut event_rx = manager.subscribe();
		gtk4::glib::MainContext::default().spawn_local(async move {
			while let Ok(event) = event_rx.recv().await {
				if matches!(event, DaemonEvent::Shutdown) {
					info!("Shutdown event received in GTK thread, stopping main loop");
					main_loop_for_shutdown.quit();
					break;
				}
			}
		});

		while let Some(cmd) = gtk_rx.recv().await {
			match cmd {
				ManagerCommand::Create {
					url,
					visible,
					profile_name: _,
					client_id,
					owner_pid,
					tx,
				} => {
					let id = Uuid::new_v4();
					let tab = Tab::new(id, &url, visible, manager.proxy_uri(), client_id, owner_pid);
					if let Some(ref win) = tab.window {
						let id_clone = id;
						let tabs_clone = std::rc::Rc::clone(&tabs);
						let event_tx_clone = manager.event_tx();
						win.connect_close_request(move |_| {
							tabs_clone.borrow_mut().remove(&id_clone);
							let _ = event_tx_clone.send(DaemonEvent::TabClosed { id: id_clone });
							gtk4::glib::Propagation::Proceed
						});
					}
					let event_tx = manager.event_tx();
					let webview = tab.webview.clone();
					webview.connect_notify_local(Some("title"), move |wv, _| {
						if let Some(title) = wv.title() {
							let _ = event_tx.send(DaemonEvent::TabTitleChanged {
								id,
								title: title.to_string(),
							});
						}
					});
					let event_tx_uri = manager.event_tx();
					let webview_uri = webview.clone();
					webview_uri.connect_notify_local(Some("uri"), move |wv, _| {
						if let Some(uri) = wv.uri() {
							let _ = event_tx_uri.send(DaemonEvent::TabNavigated {
								id,
								url: uri.to_string(),
							});
						}
					});
					tabs.borrow_mut().insert(id, tab);
					let _ = tx.send(id);
				}
				ManagerCommand::Close { id, client_id, tx } => {
					let tab_to_close = tabs.borrow_mut().remove(&id);
					let res = match tab_to_close {
						Some(tab) => {
							if tab.owner_client_id != client_id {
								tabs.borrow_mut().insert(id, tab);
								Err(ManagerError::NotOwner(id))
							} else {
								if let Some(win) = tab.window {
									win.close();
								} else {
									let _ = manager.event_tx().send(DaemonEvent::TabClosed { id });
									info!("Headless tab {} closed", id);
								}
								Ok(())
							}
						}
						None => Err(ManagerError::TabNotFound(id)),
					};
					let _ = tx.send(res);
				}
				ManagerCommand::Navigate { id, url, client_id, tx } => {
					let res = match tabs.borrow_mut().get_mut(&id) {
						Some(tab) => {
							if tab.owner_client_id != client_id {
								Err(ManagerError::NotOwner(id))
							} else {
								tab.navigate(&url);
								Ok(())
							}
						}
						None => Err(ManagerError::TabNotFound(id)),
					};
					let _ = tx.send(res);
				}
				ManagerCommand::Reload { id, client_id, tx } => {
					let res = match tabs.borrow().get(&id) {
						Some(tab) => {
							if tab.owner_client_id != client_id {
								Err(ManagerError::NotOwner(id))
							} else {
								tab.reload();
								Ok(())
							}
						}
						None => Err(ManagerError::TabNotFound(id)),
					};
					let _ = tx.send(res);
				}
				ManagerCommand::List { tx } => {
					let list = tabs
						.borrow()
						.values()
						.map(|t| {
							(
								t.id,
								t.url.clone(),
								t.title.clone(),
								t.owner_client_id,
								t.window.is_some(),
							)
						})
						.collect();
					let _ = tx.send(list);
				}
				ManagerCommand::Eval {
					id,
					script,
					client_id,
					tx,
				} => {
					let webview_opt = tabs.borrow().get(&id).and_then(|tab| {
						if tab.owner_client_id == client_id {
							Some(tab.webview.clone())
						} else {
							None
						}
					});
					match webview_opt {
						Some(webview) => {
							let eval_res = match webview.evaluate_javascript_future(&script, None, None).await {
								Ok(val) => {
									if let Some(json) = val.to_json(0) {
										Ok(json.to_string())
									} else {
										Ok(val.to_string())
									}
								}
								Err(e) => Err(ManagerError::EvalError(e.to_string())),
							};
							let _ = tx.send(eval_res);
						}
						None => {
							let err = if tabs.borrow().contains_key(&id) {
								ManagerError::NotOwner(id)
							} else {
								ManagerError::TabNotFound(id)
							};
							let _ = tx.send(Err(err));
						}
					}
				}
				ManagerCommand::SetVisible {
					id,
					visible,
					client_id,
					tx,
				} => {
					let was_visible_before;
					let res = {
						let mut tabs_borrow = tabs.borrow_mut();
						match tabs_borrow.get_mut(&id) {
							Some(tab) => {
								was_visible_before = tab.window.is_some();
								if tab.owner_client_id != client_id {
									Err(ManagerError::NotOwner(id))
								} else {
									tab.set_visible(visible)
								}
							}
							None => {
								was_visible_before = false;
								Err(ManagerError::TabNotFound(id))
							}
						}
					};
					if res.is_ok() && visible && !was_visible_before {
						let id_clone = id;
						let tabs_clone = std::rc::Rc::clone(&tabs);
						let event_tx_clone = manager.event_tx();
						if let Some(tab) = tabs.borrow().get(&id) {
							if let Some(win) = tab.window.as_ref() {
								win.connect_close_request(move |_| {
									tabs_clone.borrow_mut().remove(&id_clone);
									let _ = event_tx_clone.send(DaemonEvent::TabClosed { id: id_clone });
									gtk4::glib::Propagation::Proceed
								});
							}
						}
					}
					if res.is_ok() {
						let _ = manager
							.event_tx()
							.send(DaemonEvent::TabVisibilityChanged { id, visible });
					}
					let _ = tx.send(res);
				}
				ManagerCommand::CleanupOrphans => {
					let to_close: Vec<(Uuid, Option<gtk4::Window>)> = tabs
						.borrow()
						.iter()
						.filter(|(_, tab)| {
							if let Some(pid) = tab.owner_pid {
								!is_process_alive(pid)
							} else {
								false
							}
						})
						.map(|(id, tab)| (*id, tab.window.clone()))
						.collect();
					for (id, win_opt) in to_close {
						if let Some(win) = win_opt {
							info!("Orphan tab {} closed (owner process is dead)", id);
							win.close();
						} else {
							tabs.borrow_mut().remove(&id);
							let _ = manager.event_tx().send(DaemonEvent::TabClosed { id });
							info!("Orphan headless tab {} closed (owner process is dead)", id);
						}
					}
				}
			}
		}
	});
	main_loop.run();
	Ok(())
}
