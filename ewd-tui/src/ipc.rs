use anyhow::{anyhow, bail, Context, Result};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, Mutex};
use uuid::Uuid;

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
	jsonrpc: &'static str,
	method: String,
	params: Value,
	id: u64,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
	#[serde(default)]
	result: Option<Value>,
	#[serde(default)]
	error: Option<JsonRpcError>,
	id: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JsonRpcError {
	pub code: i64,
	pub message: String,
}

impl std::fmt::Display for JsonRpcError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "JSON-RPC error {}: {}", self.code, self.message)
	}
}

impl std::error::Error for JsonRpcError {}

type PendingRequest = oneshot::Sender<Result<Value>>;

pub struct IpcClient {
	client_id: String,
	next_id: AtomicU64,
	pending: Arc<Mutex<std::collections::HashMap<u64, PendingRequest>>>,
	_reader_handle: tokio::task::JoinHandle<()>,
	writer: Arc<Mutex<tokio::io::WriteHalf<UnixStream>>>,
}

impl IpcClient {
	pub async fn connect<P: AsRef<Path>>(socket_path: P) -> Result<(Self, mpsc::UnboundedReceiver<Value>)> {
		let stream = UnixStream::connect(socket_path.as_ref())
			.await
			.with_context(|| format!("Failed to connect to {:?}", socket_path.as_ref()))?;

		let (reader, writer) = tokio::io::split(stream);
		let client_id = Uuid::new_v4().to_string();
		debug!("IpcClient created with client_id={}", client_id);

		let pending: Arc<Mutex<std::collections::HashMap<u64, PendingRequest>>> =
			Arc::new(Mutex::new(std::collections::HashMap::new()));
		let pending_clone = pending.clone();

		let (notification_tx, notification_rx) = mpsc::unbounded_channel::<Value>();

		let reader_handle = tokio::spawn(async move {
			Self::reader_loop(reader, pending_clone, notification_tx).await;
		});

		Ok((
			Self {
				client_id,
				next_id: AtomicU64::new(1),
				pending,
				_reader_handle: reader_handle,
				writer: Arc::new(Mutex::new(writer)),
			},
			notification_rx,
		))
	}

	async fn reader_loop(
		reader: tokio::io::ReadHalf<UnixStream>,
		pending: Arc<Mutex<std::collections::HashMap<u64, PendingRequest>>>,
		notification_tx: mpsc::UnboundedSender<Value>,
	) {
		let buf_reader = BufReader::new(reader);
		let mut lines = buf_reader.lines();
		while let Ok(Some(line)) = lines.next_line().await {
			if line.trim().is_empty() {
				continue;
			}
			debug!("← {}", line);
			let raw: Value = match serde_json::from_str(&line) {
				Ok(v) => v,
				Err(e) => {
					warn!("Failed to parse JSON-RPC message: {} (line: {})", e, line);
					continue;
				}
			};
			if raw.get("id").is_some() {
				match serde_json::from_value::<JsonRpcResponse>(raw) {
					Ok(resp) => {
						if let Some(id) = resp.id {
							let mut pending_lock = pending.lock().await;
							if let Some(sender) = pending_lock.remove(&id) {
								let result = if let Some(err) = resp.error {
									Err(anyhow!("{}", err))
								} else {
									Ok(resp.result.unwrap_or(Value::Null))
								};
								let _ = sender.send(result);
							}
						}
					}
					Err(e) => warn!("Failed to parse response: {} (line: {})", e, line),
				}
			} else if let Some(method) = raw.get("method").and_then(|m| m.as_str()) {
				if method == "network_event" || method == "tabs_changed" {
					let _ = notification_tx.send(raw);
				}
			}
		}
		debug!("Reader loop terminated");
	}

	pub async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
		let id = self.next_id.fetch_add(1, Ordering::SeqCst);
		let request = JsonRpcRequest {
			jsonrpc: "2.0",
			method: method.to_string(),
			params: params.clone(),
			id,
		};
		let line = serde_json::to_string(&request)?;
		debug!("→ {}", line);

		let (tx, rx) = oneshot::channel();
		{
			let mut pending_lock = self.pending.lock().await;
			pending_lock.insert(id, tx);
		}
		{
			let mut writer = self.writer.lock().await;
			writer
				.write_all(line.as_bytes())
				.await
				.context("Failed to write to socket")?;
			writer.write_all(b"\n").await.context("Failed to write newline")?;
			writer.flush().await.context("Failed to flush socket")?;
		}

		let timeout_duration = std::time::Duration::from_secs(5);
		match tokio::time::timeout(timeout_duration, rx).await {
			Ok(Ok(result)) => result,
			Ok(Err(_)) => bail!("IPC connection closed"),
			Err(_) => {
				let mut pending_lock = self.pending.lock().await;
				pending_lock.remove(&id);
				bail!("Timeout waiting for response to method '{}'", method);
			}
		}
	}

	pub fn client_id(&self) -> &str {
		&self.client_id
	}

	pub async fn create(&self, url: &str, visible: bool) -> Result<String> {
		let params = serde_json::json!({
			"url": url,
			"visible": visible,
			"client_id": self.client_id,
		});
		let result = self.send_request("create", params).await?;
		let id = result
			.get("id")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow!("create response missing 'id' field"))?;
		Ok(id.to_string())
	}

	pub async fn close(&self, tab_id: &str) -> Result<()> {
		let params = serde_json::json!({
			"id": tab_id,
			"client_id": self.client_id,
		});
		self.send_request("close", params).await?;
		Ok(())
	}

	pub async fn show(&self, tab_id: &str) -> Result<()> {
		let params = serde_json::json!({
			"id": tab_id,
			"client_id": self.client_id,
		});
		self.send_request("show", params).await?;
		Ok(())
	}

	pub async fn hide(&self, tab_id: &str) -> Result<()> {
		let params = serde_json::json!({
			"id": tab_id,
			"client_id": self.client_id,
		});
		self.send_request("hide", params).await?;
		Ok(())
	}
}
