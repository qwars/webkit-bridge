use crate::events::DaemonEvent;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

pub type TabInfo = (Uuid, String, String, Uuid);

#[derive(Error, Debug)]
pub enum ManagerError {
	#[error("Tab not found: {0}")]
	TabNotFound(Uuid),
	#[error("Not owner of tab: {0}")]
	NotOwner(Uuid),
	#[error("Eval error: {0}")]
	EvalError(String),
	#[error("Channel error")]
	ChannelError,
}

#[derive(Debug)]
pub enum ManagerCommand {
	Create {
		url: String,
		visible: bool,
		profile_name: String,
		client_id: Uuid,
		owner_pid: Option<i32>,
		tx: oneshot::Sender<Uuid>,
	},
	Close {
		id: Uuid,
		client_id: Uuid,
		tx: oneshot::Sender<Result<(), ManagerError>>,
	},
	Navigate {
		id: Uuid,
		url: String,
		client_id: Uuid,
		tx: oneshot::Sender<Result<(), ManagerError>>,
	},
	Reload {
		id: Uuid,
		client_id: Uuid,
		tx: oneshot::Sender<Result<(), ManagerError>>,
	},
	List {
		tx: oneshot::Sender<Vec<TabInfo>>,
	},
	Eval {
		id: Uuid,
		script: String,
		client_id: Uuid,
		tx: oneshot::Sender<Result<String, ManagerError>>,
	},
	CleanupOrphans,
}

pub struct WebViewManager {
	sender: mpsc::Sender<ManagerCommand>,
	event_tx: broadcast::Sender<DaemonEvent>,
	proxy_uri: Option<Arc<String>>,
}

impl WebViewManager {
	pub fn new(proxy_uri: Option<String>) -> (Self, mpsc::Receiver<ManagerCommand>) {
		let (sender, receiver) = mpsc::channel(100);
		let (event_tx, _) = broadcast::channel(100);
		let proxy_uri = proxy_uri.map(Arc::new);

		(
			Self {
				sender,
				event_tx,
				proxy_uri,
			},
			receiver,
		)
	}

	pub fn proxy_uri(&self) -> Option<Arc<String>> {
		self.proxy_uri.clone()
	}

	pub fn event_tx(&self) -> broadcast::Sender<DaemonEvent> {
		self.event_tx.clone()
	}

	pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
		self.event_tx.subscribe()
	}

	pub async fn create_tab(
		&self,
		url: &str,
		visible: bool,
		profile_name: &str,
		client_id: Uuid,
		owner_pid: Option<i32>,
	) -> Result<Uuid, ManagerError> {
		let (tx, rx) = oneshot::channel();
		self.sender
			.send(ManagerCommand::Create {
				url: url.to_string(),
				visible,
				profile_name: profile_name.to_string(),
				client_id,
				owner_pid,
				tx,
			})
			.await
			.map_err(|_| ManagerError::ChannelError)?;

		rx.await.map_err(|_| ManagerError::ChannelError)
	}

	pub async fn close_tab(&self, id: &Uuid, client_id: Uuid) -> Result<(), ManagerError> {
		let (tx, rx) = oneshot::channel();
		self.sender
			.send(ManagerCommand::Close { id: *id, client_id, tx })
			.await
			.map_err(|_| ManagerError::ChannelError)?;

		rx.await.map_err(|_| ManagerError::ChannelError)?
	}

	pub async fn navigate_tab(&self, id: &Uuid, url: &str, client_id: Uuid) -> Result<(), ManagerError> {
		let (tx, rx) = oneshot::channel();
		self.sender
			.send(ManagerCommand::Navigate {
				id: *id,
				url: url.to_string(),
				client_id,
				tx,
			})
			.await
			.map_err(|_| ManagerError::ChannelError)?;

		rx.await.map_err(|_| ManagerError::ChannelError)?
	}

	pub async fn reload_tab(&self, id: &Uuid, client_id: Uuid) -> Result<(), ManagerError> {
		let (tx, rx) = oneshot::channel();
		self.sender
			.send(ManagerCommand::Reload { id: *id, client_id, tx })
			.await
			.map_err(|_| ManagerError::ChannelError)?;

		rx.await.map_err(|_| ManagerError::ChannelError)?
	}

	pub async fn list_tabs(&self) -> Vec<TabInfo> {
		let (tx, rx) = oneshot::channel();
		let _ = self.sender.send(ManagerCommand::List { tx }).await;
		rx.await.unwrap_or_default()
	}

	pub async fn eval_script(&self, id: &Uuid, script: &str, client_id: Uuid) -> Result<String, ManagerError> {
		let (tx, rx) = oneshot::channel();
		self.sender
			.send(ManagerCommand::Eval {
				id: *id,
				script: script.to_string(),
				client_id,
				tx,
			})
			.await
			.map_err(|_| ManagerError::ChannelError)?;

		rx.await.map_err(|_| ManagerError::ChannelError)?
	}

	pub async fn cleanup_orphans(&self) {
		let _ = self.sender.send(ManagerCommand::CleanupOrphans).await;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_manager_creation_without_proxy() {
		let (manager, _rx) = WebViewManager::new(None);
		assert!(manager.proxy_uri().is_none());
	}

	#[test]
	fn test_manager_creation_with_proxy() {
		let (manager, _rx) = WebViewManager::new(Some("http://proxy:8080".to_string()));
		let proxy = manager.proxy_uri().expect("proxy should be set");
		assert_eq!(proxy.as_str(), "http://proxy:8080");
	}

	#[test]
	fn test_proxy_uri_is_arc_shared() {
		let (manager, _rx) = WebViewManager::new(Some("http://proxy:8080".to_string()));
		let p1 = manager.proxy_uri().unwrap();
		let p2 = manager.proxy_uri().unwrap();
		assert!(Arc::ptr_eq(&p1, &p2));
	}

	#[test]
	fn test_subscribe_returns_receiver() {
		let (manager, _rx) = WebViewManager::new(None);
		let _sub = manager.subscribe();
	}

	#[test]
	fn test_event_tx_can_send_event() {
		let (manager, _rx) = WebViewManager::new(None);
		let mut sub = manager.subscribe();
		let tx = manager.event_tx();
		let event = DaemonEvent::Shutdown;
		let send_result = tx.send(event.clone());
		assert!(send_result.is_ok());
		let received = sub.try_recv().expect("should receive the event");
		assert_eq!(format!("{:?}", received), format!("{:?}", event));
	}

	#[test]
	fn test_event_tx_send_succeeds_without_subscribers() {
		let (manager, _rx) = WebViewManager::new(None);
		let tx = manager.event_tx();
		let _ = tx.send(DaemonEvent::Shutdown);
	}

	#[tokio::test]
	async fn test_manager_command_channel_delivers_create() {
		let (manager, mut rx) = WebViewManager::new(None);
		let client_id = Uuid::new_v4();
		let (tx, _resp_rx) = oneshot::channel();

		let send_result = manager
			.sender
			.send(ManagerCommand::Create {
				url: "https://example.com".to_string(),
				visible: false,
				profile_name: "default".to_string(),
				client_id,
				owner_pid: None,
				tx,
			})
			.await;
		assert!(send_result.is_ok());

		let cmd = rx.recv().await.expect("should receive command");
		match cmd {
			ManagerCommand::Create {
				url,
				visible,
				profile_name,
				client_id: cid,
				owner_pid,
				..
			} => {
				assert_eq!(url, "https://example.com");
				assert!(!visible);
				assert_eq!(profile_name, "default");
				assert_eq!(cid, client_id);
				assert!(owner_pid.is_none());
			}
			_ => panic!("Expected Create command"),
		}
	}

	#[tokio::test]
	async fn test_manager_command_channel_delivers_list() {
		let (manager, mut rx) = WebViewManager::new(None);
		let (tx, _resp_rx) = oneshot::channel();

		let send_result = manager.sender.send(ManagerCommand::List { tx }).await;
		assert!(send_result.is_ok());

		let cmd = rx.recv().await.expect("should receive command");
		assert!(matches!(cmd, ManagerCommand::List { .. }));
	}

	#[tokio::test]
	async fn test_manager_command_channel_delivers_cleanup_orphans() {
		let (manager, mut rx) = WebViewManager::new(None);
		let send_result = manager.sender.send(ManagerCommand::CleanupOrphans).await;
		assert!(send_result.is_ok());

		let cmd = rx.recv().await.expect("should receive command");
		assert!(matches!(cmd, ManagerCommand::CleanupOrphans));
	}
}
