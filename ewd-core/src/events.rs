use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEvent {
	pub url: String,
	pub method: String,
	pub status_code: Option<u32>,
	pub response_time_ms: Option<u64>,
	pub response_size_bytes: Option<u64>,
	pub event_type: NetworkEventType,
	pub request_headers: Vec<(String, String)>,
	pub response_headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkEventType {
	Started,
	Finished,
	Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonEvent {
	TabCreated { id: Uuid, url: String },
	TabClosed { id: Uuid },
	TabNavigated { id: Uuid, url: String },
	TabReloaded { id: Uuid },
	TabTitleChanged { id: Uuid, title: String },
	TabVisibilityChanged { id: Uuid, visible: bool },
	NetworkEvent { id: Uuid, event: NetworkEvent },
	Shutdown,
}

#[cfg(test)]
mod tests {
	use super::*;

	fn test_uuid() -> Uuid {
		Uuid::parse_str("12345678-1234-5678-1234-567812345678").unwrap()
	}

	#[test]
	fn test_tab_created_serializes_to_snake_case() {
		let event = DaemonEvent::TabCreated {
			id: test_uuid(),
			url: "https://example.com".to_string(),
		};
		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains("\"tab_created\""));
	}

	#[test]
	fn test_network_event_serializes_to_snake_case() {
		let event = DaemonEvent::NetworkEvent {
			id: test_uuid(),
			event: NetworkEvent {
				url: "https://example.com/api".to_string(),
				method: "GET".to_string(),
				status_code: Some(200),
				response_time_ms: Some(45),
				response_size_bytes: Some(1024),
				event_type: NetworkEventType::Finished,
				request_headers: vec![("User-Agent".to_string(), "test".to_string())],
				response_headers: vec![],
			},
		};
		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains("\"network_event\""));
		assert!(json.contains("\"finished\""));
		assert!(json.contains("\"request_headers\""));
	}

	#[test]
	fn test_roundtrip_all_variants() {
		let events = vec![
			DaemonEvent::TabCreated {
				id: test_uuid(),
				url: "https://example.com".to_string(),
			},
			DaemonEvent::TabClosed { id: test_uuid() },
			DaemonEvent::TabNavigated {
				id: test_uuid(),
				url: "https://example.com/page".to_string(),
			},
			DaemonEvent::TabReloaded { id: test_uuid() },
			DaemonEvent::TabTitleChanged {
				id: test_uuid(),
				title: "Title".to_string(),
			},
			DaemonEvent::TabVisibilityChanged {
				id: test_uuid(),
				visible: false,
			},
			DaemonEvent::NetworkEvent {
				id: test_uuid(),
				event: NetworkEvent {
					url: "https://example.com/api".to_string(),
					method: "GET".to_string(),
					status_code: Some(200),
					response_time_ms: Some(45),
					response_size_bytes: Some(1024),
					event_type: NetworkEventType::Finished,
					request_headers: vec![],
					response_headers: vec![],
				},
			},
			DaemonEvent::Shutdown,
		];
		for event in events {
			let json = serde_json::to_string(&event).unwrap();
			let deserialized: DaemonEvent = serde_json::from_str(&json).unwrap();
			assert_eq!(format!("{:?}", event), format!("{:?}", deserialized));
		}
	}

	#[test]
	fn test_deserialize_unknown_variant_fails() {
		let json = r#"{"unknown_variant": {}}"#;
		let result: Result<DaemonEvent, _> = serde_json::from_str(json);
		assert!(result.is_err(), "Unknown variant should fail to deserialize");
	}
}
