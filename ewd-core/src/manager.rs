use crate::events::DaemonEvent;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ManagerError {
	#[error("Tab not found: {0}")]
	TabNotFound(Uuid),
	#[error("Profile not found: {0}")]
	ProfileNotFound(String),
	#[error("Eval error: {0}")]
	EvalError(String),
	#[error("Not owner of tab: {0}")]
	NotOwner(Uuid),
}

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
		tx: oneshot::Sender<Vec<(Uuid, String, String, Uuid)>>,
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
		let (sender, receiver) = mpsc::channel(32);
		let (event_tx, _) = broadcast::channel(64);

		let manager = Self {
			sender,
			event_tx,
			proxy_uri: proxy_uri.map(Arc::new),
		};
		(manager, receiver)
	}

	pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
		self.event_tx.subscribe()
	}

	pub fn event_tx(&self) -> broadcast::Sender<DaemonEvent> {
		self.event_tx.clone()
	}

	pub fn proxy_uri(&self) -> Option<Arc<String>> {
		self.proxy_uri.clone()
	}

	pub async fn create_tab(
		&self,
		url: &str,
		visible: bool,
		profile_name: &str,
		client_id: Uuid,
		owner_pid: Option<i32>,
	) -> Uuid {
		let (tx, rx) = oneshot::channel();
		let _ = self
			.sender
			.send(ManagerCommand::Create {
				url: url.to_string(),
				visible,
				profile_name: profile_name.to_string(),
				client_id,
				owner_pid,
				tx,
			})
			.await;
		rx.await.expect("Failed to receive tab creation result")
	}

	pub async fn close_tab(&self, id: &Uuid, client_id: Uuid) -> Result<(), ManagerError> {
		let (tx, rx) = oneshot::channel();
		let _ = self.sender.send(ManagerCommand::Close { id: *id, client_id, tx }).await;
		rx.await.unwrap()
	}

	pub async fn navigate_tab(&self, id: &Uuid, url: &str, client_id: Uuid) -> Result<(), ManagerError> {
		let (tx, rx) = oneshot::channel();
		let _ = self
			.sender
			.send(ManagerCommand::Navigate {
				id: *id,
				url: url.to_string(),
				client_id,
				tx,
			})
			.await;
		rx.await.unwrap()
	}

	pub async fn reload_tab(&self, id: &Uuid, client_id: Uuid) -> Result<(), ManagerError> {
		let (tx, rx) = oneshot::channel();
		let _ = self
			.sender
			.send(ManagerCommand::Reload { id: *id, client_id, tx })
			.await;
		rx.await.unwrap()
	}

	pub async fn list_tabs(&self) -> Vec<(Uuid, String, String, Uuid)> {
		let (tx, rx) = oneshot::channel();
		let _ = self.sender.send(ManagerCommand::List { tx }).await;
		rx.await.unwrap()
	}

	pub async fn eval_tab(&self, id: &Uuid, script: &str, client_id: Uuid) -> Result<String, ManagerError> {
		let (tx, rx) = oneshot::channel();
		let _ = self
			.sender
			.send(ManagerCommand::Eval {
				id: *id,
				script: script.to_string(),
				client_id,
				tx,
			})
			.await;
		rx.await
			.map_err(|_| ManagerError::EvalError("Channel closed".to_string()))?
	}

	pub async fn cleanup_orphans(&self) {
		let _ = self.sender.send(ManagerCommand::CleanupOrphans).await;
	}
}
