use crate::events::{NetworkEvent, NetworkEventType};
use crate::manager::ManagerError;
use gtk4::prelude::*;
use javascriptcore6::Value as JsValue;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use uuid::Uuid;
use webkit6::prelude::*;
use webkit6::{
	NetworkProxyMode, NetworkProxySettings, UserContentInjectedFrames, UserScript, UserScriptInjectionTime, WebView,
};

pub struct Tab {
	pub id: Uuid,
	pub url: String,
	pub title: String,
	pub webview: WebView,
	pub window: Option<gtk4::Window>,
	pub owner_client_id: Uuid,
	pub owner_pid: Option<i32>,
}

impl Tab {
	pub fn new(
		id: Uuid,
		url: &str,
		visible: bool,
		proxy_uri: Option<Arc<String>>,
		owner_client_id: Uuid,
		owner_pid: Option<i32>,
	) -> Self {
		let webview = WebView::new();

		if let Some(proxy) = proxy_uri
			&& let Some(session) = webview.network_session()
		{
			let proxy_settings = NetworkProxySettings::new(Some(proxy.as_str()), &[]);
			session.set_proxy_settings(NetworkProxyMode::Custom, Some(&proxy_settings));
		}

		// НЕ вызываем load_uri здесь — это делается в start_loading()
		// после setup_js_bridge()

		let window = if visible {
			let win = gtk4::Window::new();
			win.set_title(Some(&format!("EWD Tab - {}", id)));
			win.set_default_size(1024, 768);
			win.set_child(Some(&webview));
			win.present();
			Some(win)
		} else {
			None
		};

		Self {
			id,
			url: url.to_string(),
			title: String::new(),
			webview,
			window,
			owner_client_id,
			owner_pid,
		}
	}

	/// Загружает URL в WebView. Должен вызываться ПОСЛЕ setup_js_bridge().
	pub fn start_loading(&self) {
		self.webview.load_uri(&self.url);
	}

	pub fn navigate(&mut self, url: &str) {
		self.url = url.to_string();
		self.webview.load_uri(url);
	}

	pub fn reload(&self) {
		self.webview.reload();
	}

	pub fn set_visible(&mut self, visible: bool) -> Result<(), ManagerError> {
		if visible {
			if self.window.is_none() {
				let win = gtk4::Window::new();
				win.set_title(Some(&format!("EWD Tab - {}", self.id)));
				win.set_default_size(1024, 768);
				win.set_child(Some(&self.webview));
				win.present();
				self.window = Some(win);
			} else if let Some(ref win) = self.window {
				if win.child().is_none() {
					win.set_child(Some(&self.webview));
				}
				win.present();
			}
		} else if let Some(win) = self.window.take() {
			win.set_child(None::<&gtk4::Widget>);
			win.set_visible(false);
		}
		Ok(())
	}

	pub fn setup_network_monitoring<F>(&self, callback: F)
	where
		F: Fn(NetworkEvent) + Send + Sync + 'static,
	{
		let timings = Arc::new(Mutex::new(HashMap::<String, Instant>::new()));
		let callback = Arc::new(Mutex::new(callback));

		self.webview
			.connect_resource_load_started(move |_webview, resource, request| {
				let uri = request
					.uri()
					.map(|u| u.to_string())
					.unwrap_or_else(|| "unknown".to_string());
				let method = request
					.http_method()
					.map(|m| m.to_string())
					.unwrap_or_else(|| "GET".to_string());

				let mut req_headers = Vec::new();
				if let Some(headers) = request.http_headers() {
					headers.foreach(|name, value| {
						req_headers.push((name.to_string(), value.to_string()));
					});
				}

				let req_headers_arc = Arc::new(req_headers);

				{
					let mut map = timings.lock().unwrap();
					map.insert(uri.clone(), Instant::now());
				}

				{
					let cb = callback.lock().unwrap();
					cb(NetworkEvent {
						url: uri.clone(),
						method: method.clone(),
						status_code: None,
						response_time_ms: None,
						response_size_bytes: None,
						event_type: NetworkEventType::Started,
						request_headers: (*req_headers_arc).clone(),
						response_headers: Vec::new(),
					});
				}

				// --- Обработчик Finished ---
				let timings_finished = Arc::clone(&timings);
				let uri_finished = uri.clone();
				let method_finished = method.clone();
				let req_headers_finished = Arc::clone(&req_headers_arc);
				let callback_finished = Arc::clone(&callback);

				resource.connect_finished(move |res| {
					let response_time_ms = {
						let mut map = timings_finished.lock().unwrap();
						map.remove(&uri_finished).map(|t| t.elapsed().as_millis() as u64)
					};

					let status_code = res.response().map(|r| r.status_code());

					let response_size_bytes = res.response().and_then(|r| {
						let len = r.content_length();
						if len > 0 { Some(len) } else { None }
					});

					let mut resp_headers = Vec::new();
					if let Some(response) = res.response()
						&& let Some(headers) = response.http_headers()
					{
						headers.foreach(|name, value| {
							resp_headers.push((name.to_string(), value.to_string()));
						});
					}

					let cb = callback_finished.lock().unwrap();
					cb(NetworkEvent {
						url: uri_finished.clone(),
						method: method_finished.clone(),
						status_code,
						response_time_ms,
						response_size_bytes,
						event_type: NetworkEventType::Finished,
						request_headers: (*req_headers_finished).clone(),
						response_headers: resp_headers,
					});
				});

				// --- Обработчик Failed ---
				let timings_failed = Arc::clone(&timings);
				let uri_failed = uri.clone();
				let method_failed = method.clone();
				let req_headers_failed = Arc::clone(&req_headers_arc);
				let callback_failed = Arc::clone(&callback);

				resource.connect_failed(move |_res, _err| {
					let response_time_ms = {
						let mut map = timings_failed.lock().unwrap();
						map.remove(&uri_failed).map(|t| t.elapsed().as_millis() as u64)
					};

					let cb = callback_failed.lock().unwrap();
					cb(NetworkEvent {
						url: uri_failed.clone(),
						method: method_failed.clone(),
						status_code: None,
						response_time_ms, // ← используем вычисленную переменную
						response_size_bytes: None,
						event_type: NetworkEventType::Failed,
						request_headers: (*req_headers_failed).clone(),
						response_headers: Vec::new(),
					});
				});
			});
	}

	pub fn setup_js_bridge<F>(&self, callback: F)
	where
		F: Fn(String, serde_json::Value) + Send + Sync + 'static,
	{
		let Some(manager) = self.webview.user_content_manager() else {
			tracing::warn!(
				"Failed to get UserContentManager for tab {}, JS bridge not installed",
				self.id
			);
			return;
		};

		// 1. Регистрируем handler "ewd" для приёма сообщений из JS
		if !manager.register_script_message_handler("ewd", None) {
			tracing::error!("Failed to register script message handler 'ewd' for tab {}", self.id);
			return;
		}

		// 2. Подключаемся к событию получения сообщения
		let callback = Arc::new(Mutex::new(callback));
		let callback_clone = Arc::clone(&callback);
		let tab_id = self.id;

		manager.connect_script_message_received(Some("ewd"), move |_, message: &JsValue| {
			let raw = message.to_str().to_string();
			tracing::debug!("Received JS message in tab {}: {}", tab_id, raw);

			match serde_json::from_str::<serde_json::Value>(&raw) {
				Ok(parsed) => {
					let message_type = parsed
						.get("type")
						.and_then(|v| v.as_str())
						.unwrap_or("unknown")
						.to_string();
					let data = parsed.get("data").cloned().unwrap_or(serde_json::Value::Null);

					let cb = callback_clone.lock().unwrap();
					cb(message_type, data);
				}
				Err(e) => {
					tracing::warn!(
						"Failed to parse JS message as JSON in tab {}: {} (raw: {})",
						tab_id,
						e,
						raw
					);
				}
			}
		});

		// 3. Инжектируем JS-мост во все фреймы в момент начала загрузки страницы
		let js_bridge = r#"
            (function() {
                // Создаём window.ewd если его нет, или используем существующий
                window.ewd = window.ewd || {};
                // Всегда инжектим метод send, даже если window.ewd уже существовал
                window.ewd.send = function(type, data) {
                    try {
                        if (window.webkit &&
                            window.webkit.messageHandlers &&
                            window.webkit.messageHandlers.ewd) {
                            window.webkit.messageHandlers.ewd.postMessage(
                                JSON.stringify({ type: type, data: data })
                            );
                        } else {
                            console.error('[ewd] message handler unavailable');
                        }
                    } catch (e) {
                        console.error('[ewd] send error', e);
                    }
                };
            })();
        "#;

		manager.add_script(&UserScript::new(
			js_bridge,
			UserContentInjectedFrames::AllFrames,
			UserScriptInjectionTime::Start,
			&[],
			&[],
		));

		tracing::info!("JS bridge installed for tab {}", self.id);
	}
}
