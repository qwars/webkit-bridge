use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonEvent {
	TabCreated { id: Uuid, url: String },
	TabClosed { id: Uuid },
	TabNavigated { id: Uuid, url: String },
	TabReloaded { id: Uuid },
	TabTitleChanged { id: Uuid, title: String },
	Shutdown,
}
