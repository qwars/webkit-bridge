use crate::events::DaemonEvent;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

pub type TabInfo = (Uuid, String, String, Uuid, bool);

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
	SetVisible {
		id: Uuid,
		visible: bool,
		client_id: Uuid,
		tx: oneshot::Sender<Result<(), ManagerError>>,
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

	pub async fn set_tab_visible(&self, id: &Uuid, visible: bool, client_id: Uuid) -> Result<(), ManagerError> {
		let (tx, rx) = oneshot::channel();
		self.sender
			.send(ManagerCommand::SetVisible {
				id: *id,
				visible,
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
