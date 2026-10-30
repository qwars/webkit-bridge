use crate::manager::ManagerError;
use gtk4::prelude::*;
use std::sync::Arc;
use uuid::Uuid;
use webkit6::prelude::*;
use webkit6::{NetworkProxyMode, NetworkProxySettings, WebView};

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
		webview.load_uri(url);
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
}
