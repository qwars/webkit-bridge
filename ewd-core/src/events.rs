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
	TabVisibilityChanged { id: Uuid, visible: bool },
	Shutdown,
}

#[cfg(test)]
mod tests {
	use super::*;
	use uuid::Uuid;

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
		assert!(
			json.contains("\"tab_created\""),
			"Expected snake_case variant name, got: {}",
			json
		);
		assert!(json.contains("12345678-1234-5678-1234-567812345678"));
		assert!(json.contains("https://example.com"));
	}

	#[test]
	fn test_tab_closed_serializes_to_snake_case() {
		let event = DaemonEvent::TabClosed { id: test_uuid() };
		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains("\"tab_closed\""));
	}

	#[test]
	fn test_tab_navigated_serializes_to_snake_case() {
		let event = DaemonEvent::TabNavigated {
			id: test_uuid(),
			url: "https://example.com/page".to_string(),
		};
		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains("\"tab_navigated\""));
	}

	#[test]
	fn test_tab_reloaded_serializes_to_snake_case() {
		let event = DaemonEvent::TabReloaded { id: test_uuid() };
		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains("\"tab_reloaded\""));
	}

	#[test]
	fn test_tab_title_changed_serializes_to_snake_case() {
		let event = DaemonEvent::TabTitleChanged {
			id: test_uuid(),
			title: "Hello World".to_string(),
		};
		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains("\"tab_title_changed\""));
		assert!(json.contains("Hello World"));
	}

	#[test]
	fn test_shutdown_serializes_to_snake_case() {
		let event = DaemonEvent::Shutdown;
		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains("\"shutdown\""));
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
	#[test]
	fn test_tab_visibility_changed_serializes_to_snake_case() {
		let event = DaemonEvent::TabVisibilityChanged {
			id: test_uuid(),
			visible: true,
		};
		let json = serde_json::to_string(&event).unwrap();
		assert!(
			json.contains("\"tab_visibility_changed\""),
			"Expected snake_case variant name, got: {}",
			json
		);
		assert!(json.contains("12345678-1234-5678-1234-567812345678"));
	}
}
