use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

const CLI_CLIENT_ID: &str = "00000000-0000-0000-0000-000000000000";

#[derive(Parser)]
#[command(name = "wkctl")]
#[command(about = "CLI client for EWD (Emacs WebKit Daemon)")]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	#[command(name = "create")]
	Create {
		#[arg(long, help = "URL to load in the tab")]
		url: String,
		#[arg(long, help = "Make the tab visible in a GTK window")]
		visible: bool,
	},
	#[command(name = "close")]
	Close {
		#[arg(long, help = "Tab ID to close")]
		id: String,
	},
	#[command(name = "navigate")]
	Navigate {
		#[arg(long, help = "Tab ID to navigate")]
		id: String,
		#[arg(long, help = "URL to navigate to")]
		url: String,
	},
	#[command(name = "reload")]
	Reload {
		#[arg(long, help = "Tab ID to reload")]
		id: String,
	},
	#[command(name = "list")]
	List,
	#[command(name = "eval")]
	Eval {
		#[arg(long, help = "Tab ID to evaluate script in")]
		id: String,
		#[arg(long, help = "JavaScript code to evaluate")]
		script: String,
	},
	#[command(name = "show")]
	Show {
		#[arg(long, help = "Tab ID to show in a GTK window")]
		id: String,
	},
	#[command(name = "hide")]
	Hide {
		#[arg(long, help = "Tab ID to hide (convert to headless)")]
		id: String,
	},
}

#[derive(serde::Serialize)]
struct JsonRpcRequest {
	jsonrpc: String,
	method: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	params: Option<serde_json::Value>,
	id: u64,
}

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)]
struct JsonRpcResponse {
	jsonrpc: String,
	result: Option<serde_json::Value>,
	error: Option<JsonRpcError>,
	id: Option<u64>,
}

#[derive(serde::Deserialize, Debug)]
struct JsonRpcError {
	code: i32,
	message: String,
}

fn resolve_socket_path(ewd_socket: Option<&str>, xdg_runtime: Option<&str>) -> Option<String> {
	if let Some(path) = ewd_socket
		&& !path.is_empty()
	{
		return Some(path.to_string());
	}
	if let Some(dir) = xdg_runtime
		&& !dir.is_empty()
	{
		return Some(format!("{dir}/webkit-bridge.sock"));
	}
	None
}

fn get_socket_path() -> String {
	let ewd = env::var("EWD_SOCKET_PATH").ok();
	let xdg = env::var("XDG_RUNTIME_DIR").ok();
	resolve_socket_path(ewd.as_deref(), xdg.as_deref()).unwrap_or_else(|| {
		eprintln!("Error: Neither EWD_SOCKET_PATH nor XDG_RUNTIME_DIR is set. Cannot determine socket path.");
		std::process::exit(1);
	})
}

fn send_request(method: &str, params: Option<serde_json::Value>) -> Result<(), Box<dyn std::error::Error>> {
	let socket_path = get_socket_path();
	let mut stream = UnixStream::connect(&socket_path).map_err(|e| {
		format!(
			"Failed to connect to daemon at {}. Is it running?\nError: {}",
			socket_path, e
		)
	})?;

	let mut req_params = params.unwrap_or(json!({}));
	if let Value::Object(ref mut map) = req_params {
		map.insert("client_id".to_string(), Value::String(CLI_CLIENT_ID.to_string()));
	}

	let request = JsonRpcRequest {
		jsonrpc: "2.0".to_string(),
		method: method.to_string(),
		params: Some(req_params),
		id: 1,
	};

	let request_str = serde_json::to_string(&request)? + "\n";
	stream.write_all(request_str.as_bytes())?;
	stream.flush()?;

	let mut reader = BufReader::new(stream);
	let mut response_str = String::new();

	let response: JsonRpcResponse = loop {
		response_str.clear();
		let bytes = reader.read_line(&mut response_str)?;
		if bytes == 0 {
			return Err("Unexpected EOF from daemon".into());
		}
		if response_str.trim().is_empty() {
			continue;
		}
		if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(&response_str)
			&& resp.id == Some(1) {
				break resp;
			}
	};

	if let Some(error) = response.error {
		eprintln!("Error [{}]: {}", error.code, error.message);
		std::process::exit(1);
	}

	if let Some(result) = response.result {
		print_response(result);
	} else {
		println!("OK");
	}

	Ok(())
}

fn format_response(result: serde_json::Value) -> String {
	if result.is_null() {
		"OK".to_string()
	} else if let Some(obj) = result.as_object() {
		if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
			format!("Tab created: {}", id)
		} else if let Some(res) = obj.get("result") {
			if let Some(s) = res.as_str() {
				s.to_string()
			} else {
				res.to_string()
			}
		} else {
			result.to_string()
		}
	} else if let Some(array) = result.as_array() {
		if array.is_empty() {
			"No tabs open".to_string()
		} else {
			let mut output = String::from("Open tabs:\n");
			for item in array {
				if let Some(obj) = item.as_object() {
					let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
					let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("unknown");
					let title = obj.get("title").and_then(|v| v.as_str()).unwrap_or(" ");
					let owner = obj.get("owner").and_then(|v| v.as_str()).unwrap_or("unknown");
					output.push_str(&format!("  {} - {} ({}) [owner: {}]\n", id, url, title, owner));
				}
			}
			output.trim_end().to_string()
		}
	} else {
		result.to_string()
	}
}

fn print_response(result: serde_json::Value) {
	println!("{}", format_response(result));
}

fn main() {
	let cli = Cli::parse();

	let result = match cli.command {
		Commands::Create { url, visible } => send_request("create", Some(json!({ "url": url, "visible": visible }))),
		Commands::Close { id } => send_request("close", Some(json!({ "id": id }))),
		Commands::Navigate { id, url } => send_request("navigate", Some(json!({ "id": id, "url": url }))),
		Commands::Reload { id } => send_request("reload", Some(json!({ "id": id }))),
		Commands::List => send_request("list", None),
		Commands::Eval { id, script } => send_request("eval", Some(json!({ "id": id, "script": script }))),
		Commands::Show { id } => send_request("show", Some(json!({ "id": id }))),
		Commands::Hide { id } => send_request("hide", Some(json!({ "id": id }))),
	};

	if let Err(e) = result {
		eprintln!("{}", e);
		std::process::exit(1);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_resolve_socket_path_ewd_has_priority() {
		let path = resolve_socket_path(Some("/tmp/custom.sock"), Some("/run/user/1000"));
		assert_eq!(path, Some("/tmp/custom.sock".to_string()));
	}

	#[test]
	fn test_resolve_socket_path_falls_back_to_xdg() {
		let path = resolve_socket_path(None, Some("/run/user/1000"));
		assert_eq!(path, Some("/run/user/1000/webkit-bridge.sock".to_string()));
	}

	#[test]
	fn test_resolve_socket_path_returns_none_when_both_missing() {
		let path = resolve_socket_path(None, None);
		assert_eq!(path, None);
	}

	#[test]
	fn test_resolve_socket_path_ignores_empty_ewd() {
		let path = resolve_socket_path(Some(""), Some("/run/user/1000"));
		assert_eq!(path, Some("/run/user/1000/webkit-bridge.sock".to_string()));
	}

	#[test]
	fn test_format_response_null() {
		let result = format_response(serde_json::Value::Null);
		assert_eq!(result, "OK");
	}

	#[test]
	fn test_format_response_tab_created() {
		let result = format_response(serde_json::json!({"id": "test-uuid"}));
		assert_eq!(result, "Tab created: test-uuid");
	}

	#[test]
	fn test_format_response_list_empty() {
		let result = format_response(serde_json::json!([]));
		assert_eq!(result, "No tabs open");
	}

	#[test]
	fn test_format_response_list_with_tabs() {
		let result = format_response(serde_json::json!([
			{"id": "uuid1", "url": "https://example.com", "title": "Example", "owner": "owner1"}
		]));
		assert!(result.contains("uuid1"));
		assert!(result.contains("https://example.com"));
		assert!(result.contains("Example"));
		assert!(result.contains("owner1"));
	}

	#[test]
	fn test_format_response_eval_result() {
		let result = format_response(serde_json::json!({"result": "42"}));
		assert_eq!(result, "42");
	}
}
