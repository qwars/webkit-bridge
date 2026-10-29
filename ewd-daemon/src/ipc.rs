use anyhow::Result;
use ewd_core::events::DaemonEvent;
use ewd_core::manager::WebViewManager;
use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
	pub jsonrpc: String,
	pub method: String,
	#[serde(default)]
	pub params: Option<serde_json::Value>,
	#[serde(default)]
	pub id: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
	pub jsonrpc: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub result: Option<serde_json::Value>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<JsonRpcError>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub id: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
	pub code: i32,
	pub message: String,
}

fn short_uuid(id: &Uuid) -> String {
	id.to_string().chars().take(8).collect()
}

pub struct IpcServer {
	socket_path: String,
	manager: Arc<WebViewManager>,
}

impl IpcServer {
	pub fn new(socket_path: &str, manager: Arc<WebViewManager>) -> Self {
		Self {
			socket_path: socket_path.to_string(),
			manager,
		}
	}

	pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
		self.manager.subscribe()
	}

	pub fn broadcast_event(&self, event: DaemonEvent) {
		let _ = self.manager.event_tx().send(event);
	}

	pub async fn start(&self) -> Result<()> {
		let listener = UnixListener::bind(&self.socket_path)?;
		info!("IPC server listening on {}", self.socket_path);
		loop {
			match listener.accept().await {
				Ok((stream, _)) => {
					let client_id = Uuid::new_v4();
					let pid = getsockopt(&stream, PeerCredentials).ok().map(|cred| cred.pid());
					info!("New client connected: {} (PID: {:?})", short_uuid(&client_id), pid);
					let manager = Arc::clone(&self.manager);
					tokio::spawn(async move {
						if let Err(e) = Self::handle_client(stream, manager, client_id, pid).await {
							warn!("Client handler error: {}", e);
						}
					});
				}
				Err(e) => {
					warn!("Failed to accept connection: {}", e);
				}
			}
		}
	}

	async fn handle_client(
		stream: UnixStream,
		manager: Arc<WebViewManager>,
		session_client_id: Uuid,
		owner_pid: Option<i32>,
	) -> Result<()> {
		let (reader, mut writer) = stream.into_split();
		let mut reader = BufReader::new(reader);
		let mut line = String::new();
		loop {
			line.clear();
			let bytes_read = reader.read_line(&mut line).await?;
			if bytes_read == 0 {
				info!("Client disconnected: {}", short_uuid(&session_client_id));
				break;
			}
			if line.trim().is_empty() {
				continue;
			}
			let request: JsonRpcRequest = match serde_json::from_str::<JsonRpcRequest>(line.trim()) {
				Ok(req) if req.jsonrpc == "2.0" => req,
				_ => {
					warn!("Invalid JSON-RPC request received: {}", line.trim());
					let response = JsonRpcResponse {
						jsonrpc: "2.0".to_string(),
						result: None,
						error: Some(JsonRpcError {
							code: -32600,
							message: "Invalid Request".to_string(),
						}),
						id: None,
					};
					let response_json = serde_json::to_string(&response)?;
					writer.write_all(response_json.as_bytes()).await?;
					writer.write_all(b"\n").await?;
					continue;
				}
			};
			let response = Self::process_request(request, &manager, session_client_id, owner_pid).await;
			let response_json = serde_json::to_string(&response)?;
			writer.write_all(response_json.as_bytes()).await?;
			writer.write_all(b"\n").await?;
		}
		Ok(())
	}

	fn error_response(code: i32, message: String, id: Option<serde_json::Value>) -> JsonRpcResponse {
		JsonRpcResponse {
			jsonrpc: "2.0".to_string(),
			result: None,
			error: Some(JsonRpcError { code, message }),
			id,
		}
	}

	async fn process_request(
		request: JsonRpcRequest,
		manager: &Arc<WebViewManager>,
		session_client_id: Uuid,
		owner_pid: Option<i32>,
	) -> JsonRpcResponse {
		let effective_client_id = request
			.params
			.as_ref()
			.and_then(|p| p.get("client_id"))
			.and_then(|v| v.as_str())
			.and_then(|s| Uuid::parse_str(s).ok())
			.unwrap_or(session_client_id);

		info!(
			"Processing RPC method: {} from client {}",
			request.method,
			short_uuid(&effective_client_id)
		);

		let event_tx = manager.event_tx();

		match request.method.as_str() {
			"tab.create" => {
				let url = request
					.params
					.as_ref()
					.and_then(|p| p.get("url").and_then(|v| v.as_str()))
					.unwrap_or("about:blank")
					.to_string();
				let visible = request
					.params
					.as_ref()
					.and_then(|p| p.get("visible").and_then(|v| v.as_bool()))
					.unwrap_or(false);
				let profile = request
					.params
					.as_ref()
					.and_then(|p| p.get("profile").and_then(|v| v.as_str()))
					.unwrap_or("default")
					.to_string();
				match manager
					.create_tab(&url, visible, &profile, effective_client_id, owner_pid)
					.await
				{
					Ok(id) => {
						let _ = event_tx.send(DaemonEvent::TabCreated { id, url: url.clone() });
						info!(
							"Tab created successfully: {} by client {}",
							id,
							short_uuid(&effective_client_id)
						);
						JsonRpcResponse {
							jsonrpc: "2.0".to_string(),
							result: Some(serde_json::json!({"id": id.to_string()})),
							error: None,
							id: request.id,
						}
					}
					Err(e) => {
						warn!("Failed to create tab: {}", e);
						Self::error_response(-32000, e.to_string(), request.id)
					}
				}
			}
			"tab.close" => {
				let id_str = request
					.params
					.as_ref()
					.and_then(|p| p.get("id").and_then(|v| v.as_str()))
					.unwrap_or("")
					.to_string();
				let tab_id = match Uuid::parse_str(&id_str) {
					Ok(id) => id,
					Err(_) => {
						return Self::error_response(-32602, "Invalid params: invalid UUID".to_string(), request.id);
					}
				};
				match manager.close_tab(&tab_id, effective_client_id).await {
					Ok(()) => {
						info!("Tab closed successfully: {}", tab_id);
						JsonRpcResponse {
							jsonrpc: "2.0".to_string(),
							result: Some(serde_json::Value::Null),
							error: None,
							id: request.id,
						}
					}
					Err(e) => {
						warn!("Failed to close tab {}: {}", tab_id, e);
						Self::error_response(-32000, e.to_string(), request.id)
					}
				}
			}
			"tab.navigate" => {
				let id_str = request
					.params
					.as_ref()
					.and_then(|p| p.get("id").and_then(|v| v.as_str()))
					.unwrap_or("")
					.to_string();
				let url = request
					.params
					.as_ref()
					.and_then(|p| p.get("url").and_then(|v| v.as_str()))
					.unwrap_or("about:blank")
					.to_string();
				let tab_id = match Uuid::parse_str(&id_str) {
					Ok(id) => id,
					Err(_) => {
						return Self::error_response(-32602, "Invalid params: invalid UUID".to_string(), request.id);
					}
				};
				match manager.navigate_tab(&tab_id, &url, effective_client_id).await {
					Ok(()) => {
						let _ = event_tx.send(DaemonEvent::TabNavigated {
							id: tab_id,
							url: url.clone(),
						});
						info!("Tab {} navigated to {}", tab_id, url);
						JsonRpcResponse {
							jsonrpc: "2.0".to_string(),
							result: Some(serde_json::Value::Null),
							error: None,
							id: request.id,
						}
					}
					Err(e) => {
						warn!("Failed to navigate tab {}: {}", tab_id, e);
						Self::error_response(-32000, e.to_string(), request.id)
					}
				}
			}
			"tab.reload" => {
				let id_str = request
					.params
					.as_ref()
					.and_then(|p| p.get("id").and_then(|v| v.as_str()))
					.unwrap_or("")
					.to_string();
				let tab_id = match Uuid::parse_str(&id_str) {
					Ok(id) => id,
					Err(_) => {
						return Self::error_response(-32602, "Invalid params: invalid UUID".to_string(), request.id);
					}
				};
				match manager.reload_tab(&tab_id, effective_client_id).await {
					Ok(()) => {
						let _ = event_tx.send(DaemonEvent::TabReloaded { id: tab_id });
						info!("Tab {} reloaded", tab_id);
						JsonRpcResponse {
							jsonrpc: "2.0".to_string(),
							result: Some(serde_json::Value::Null),
							error: None,
							id: request.id,
						}
					}
					Err(e) => {
						warn!("Failed to reload tab {}: {}", tab_id, e);
						Self::error_response(-32000, e.to_string(), request.id)
					}
				}
			}
			"tab.eval" => {
				let id_str = request
					.params
					.as_ref()
					.and_then(|p| p.get("id").and_then(|v| v.as_str()))
					.unwrap_or("")
					.to_string();
				let script = request
					.params
					.as_ref()
					.and_then(|p| p.get("script").and_then(|v| v.as_str()))
					.unwrap_or("")
					.to_string();
				let tab_id = match Uuid::parse_str(&id_str) {
					Ok(id) => id,
					Err(_) => {
						return Self::error_response(-32602, "Invalid params: invalid UUID".to_string(), request.id);
					}
				};
				match manager.eval_script(&tab_id, &script, effective_client_id).await {
					Ok(result) => {
						info!("Evaluated JS successfully in tab {}", tab_id);
						JsonRpcResponse {
							jsonrpc: "2.0".to_string(),
							result: Some(serde_json::json!({"result": result})),
							error: None,
							id: request.id,
						}
					}
					Err(e) => {
						warn!("JS evaluation failed in tab {}: {}", tab_id, e);
						Self::error_response(-32000, e.to_string(), request.id)
					}
				}
			}
			"tab.list" => {
				let tabs = manager.list_tabs().await;
				let tabs_json: Vec<_> = tabs
					.into_iter()
					.map(|(id, url, title, owner)| {
						serde_json::json!({
							"id": id.to_string(),
							"url": url,
							"title": title,
							"owner": short_uuid(&owner)
						})
					})
					.collect();
				info!("Listed {} tabs", tabs_json.len());
				JsonRpcResponse {
					jsonrpc: "2.0".to_string(),
					result: Some(serde_json::Value::Array(tabs_json)),
					error: None,
					id: request.id,
				}
			}
			"daemon.shutdown" => {
				info!("Shutdown requested by client {}", short_uuid(&effective_client_id));
				let _ = event_tx.send(DaemonEvent::Shutdown);
				JsonRpcResponse {
					jsonrpc: "2.0".to_string(),
					result: Some(serde_json::Value::Null),
					error: None,
					id: request.id,
				}
			}
			_ => {
				warn!("Unknown method requested: {}", request.method);
				Self::error_response(-32601, format!("Method not found: {}", request.method), request.id)
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_short_uuid_returns_8_chars() {
		let uuid = Uuid::parse_str("12345678-1234-5678-1234-567812345678").unwrap();
		let short = short_uuid(&uuid);
		assert_eq!(short, "12345678");
		assert_eq!(short.len(), 8);
	}

	#[test]
	fn test_jsonrpc_request_parse_valid() {
		let json = r#"{"jsonrpc":"2.0","method":"tab.create","params":{"url":"https://example.com"},"id":1}"#;
		let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
		assert_eq!(req.jsonrpc, "2.0");
		assert_eq!(req.method, "tab.create");
		assert!(req.params.is_some());
		assert!(req.id.is_some());
	}

	#[test]
	fn test_jsonrpc_request_parse_without_params() {
		let json = r#"{"jsonrpc":"2.0","method":"tab.list","id":2}"#;
		let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
		assert_eq!(req.method, "tab.list");
		assert!(req.params.is_none());
	}

	#[test]
	fn test_jsonrpc_request_parse_invalid_json() {
		let json = r#"{"jsonrpc":"2.0","method":"tab.create""#;
		let result: Result<JsonRpcRequest, _> = serde_json::from_str(json);
		assert!(result.is_err());
	}

	#[test]
	fn test_error_response_format() {
		let resp = JsonRpcResponse {
			jsonrpc: "2.0".to_string(),
			result: None,
			error: Some(JsonRpcError {
				code: -32600,
				message: "Invalid Request".to_string(),
			}),
			id: Some(serde_json::json!(1)),
		};
		assert_eq!(resp.jsonrpc, "2.0");
		assert!(resp.result.is_none());
		assert!(resp.error.is_some());
		let err = resp.error.unwrap();
		assert_eq!(err.code, -32600);
		assert_eq!(err.message, "Invalid Request");
	}

	#[test]
	fn test_jsonrpc_response_serialize_success() {
		let resp = JsonRpcResponse {
			jsonrpc: "2.0".to_string(),
			result: Some(serde_json::json!({"id": "test-uuid"})),
			error: None,
			id: Some(serde_json::json!(1)),
		};
		let json = serde_json::to_string(&resp).unwrap();
		assert!(json.contains("\"result\""));
		assert!(!json.contains("\"error\""));
	}

	#[test]
	fn test_jsonrpc_response_serialize_error() {
		let resp = JsonRpcResponse {
			jsonrpc: "2.0".to_string(),
			result: None,
			error: Some(JsonRpcError {
				code: -32601,
				message: "Method not found".to_string(),
			}),
			id: Some(serde_json::json!(2)),
		};
		let json = serde_json::to_string(&resp).unwrap();
		assert!(json.contains("\"error\""));
		assert!(!json.contains("\"result\""));
	}
}
