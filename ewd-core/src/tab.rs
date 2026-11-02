use crate::events::{NetworkEvent, NetworkEventType};
use crate::manager::ManagerError;
use gtk4::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
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
		if let Some(proxy) = proxy_uri {
			if let Some(session) = webview.network_session() {
				let proxy_settings = NetworkProxySettings::new(Some(proxy.as_str()), &[]);
				session.set_proxy_settings(NetworkProxyMode::Custom, Some(&proxy_settings));
			}
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

	/// Подключает мониторинг сетевых запросов для этого таба.
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

				// Извлекаем заголовки запроса (foreach — встроенный метод, импорт не нужен)
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

					let mut resp_headers = Vec::new();
					if let Some(response) = res.response() {
						if let Some(headers) = response.http_headers() {
							headers.foreach(|name, value| {
								resp_headers.push((name.to_string(), value.to_string()));
							});
						}
					}

					let cb = callback_finished.lock().unwrap();
					cb(NetworkEvent {
						url: uri_finished.clone(),
						method: method_finished.clone(),
						status_code,
						response_time_ms,
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
						response_time_ms,
						event_type: NetworkEventType::Failed,
						request_headers: (*req_headers_failed).clone(),
						response_headers: Vec::new(),
					});
				});
			});
	}
}
