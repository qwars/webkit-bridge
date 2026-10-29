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
	#[command(name = "tab.create")]
	Create {
		#[arg(long, help = "URL to load in the tab")]
		url: String,
		#[arg(long, help = "Make the tab visible in a GTK window")]
		visible: bool,
	},
	#[command(name = "tab.close")]
	Close {
		#[arg(long, help = "Tab ID to close")]
		id: String,
	},
	#[command(name = "tab.navigate")]
	Navigate {
		#[arg(long, help = "Tab ID to navigate")]
		id: String,
		#[arg(long, help = "URL to navigate to")]
		url: String,
	},
	#[command(name = "tab.reload")]
	Reload {
		#[arg(long, help = "Tab ID to reload")]
		id: String,
	},
	#[command(name = "tab.list")]
	List,
	#[command(name = "tab.eval")]
	Eval {
		#[arg(long, help = "Tab ID to evaluate script in")]
		id: String,
		#[arg(long, help = "JavaScript code to evaluate")]
		script: String,
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

fn get_socket_path() -> String {
	env::var("EWD_SOCKET_PATH").unwrap_or_else(|_| {
		env::var("XDG_RUNTIME_DIR")
			.map(|dir| format!("{}/webkit-bridge.sock", dir))
			.unwrap_or_else(|_| "/tmp/webkit-bridge.sock".to_string())
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
	reader.read_line(&mut response_str)?;

	let response: JsonRpcResponse = serde_json::from_str(&response_str)?;

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
		// ИСПРАВЛЕНО: используем as_str() для извлечения строки без кавычек
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
		Commands::Create { url, visible } => {
			send_request("tab.create", Some(json!({ "url": url, "visible": visible })))
		}
		Commands::Close { id } => send_request("tab.close", Some(json!({ "id": id }))),
		Commands::Navigate { id, url } => send_request("tab.navigate", Some(json!({ "id": id, "url": url }))),
		Commands::Reload { id } => send_request("tab.reload", Some(json!({ "id": id }))),
		Commands::List => send_request("tab.list", None),
		Commands::Eval { id, script } => send_request("tab.eval", Some(json!({ "id": id, "script": script }))),
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
	fn test_get_socket_path_with_ewd_socket_path() {
		// SAFETY: Тесты не запускаются параллельно с production кодом, который читает EWD_SOCKET_PATH.
		// Мы контролируем порядок выполнения и гарантируем отсутствие data race.
		unsafe {
			env::set_var("EWD_SOCKET_PATH", "/tmp/test.sock");
		}
		let path = get_socket_path();
		assert_eq!(path, "/tmp/test.sock");
		unsafe {
			env::remove_var("EWD_SOCKET_PATH");
		}
	}

	#[test]
	fn test_get_socket_path_without_ewd_socket_path() {
		// SAFETY: Тесты не запускаются параллельно с production кодом, который читает XDG_RUNTIME_DIR.
		// Мы контролируем порядок выполнения и гарантируем отсутствие data race.
		unsafe {
			env::remove_var("EWD_SOCKET_PATH");
			env::remove_var("XDG_RUNTIME_DIR");
		}
		let path = get_socket_path();
		assert_eq!(path, "/tmp/webkit-bridge.sock");
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
