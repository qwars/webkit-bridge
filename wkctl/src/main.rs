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
	Create {
		url: String,
		#[arg(long, help = "Make the tab visible in a GTK window")]
		visible: bool,
	},
	Close {
		id: String,
	},
	Navigate {
		id: String,
		url: String,
	},
	Reload {
		id: String,
	},
	List,
	Eval {
		id: String,
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
	if let Ok(xdg) = env::var("XDG_RUNTIME_DIR") {
		format!("{}/webkit-bridge.sock", xdg)
	} else {
		"/tmp/webkit-bridge.sock".to_string()
	}
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
	}

	Ok(())
}

fn print_response(result: serde_json::Value) {
	if result.is_null() {
		println!("OK");
	} else if let Some(obj) = result.as_object() {
		if let Some(id) = obj.get("id") {
			println!("Tab created: {}", id);
		} else if let Some(res) = obj.get("result") {
			if let Some(s) = res.as_str() {
				println!("{}", s);
			} else {
				println!("{}", res);
			}
		} else {
			println!("{}", result);
		}
	} else if let Some(array) = result.as_array() {
		if array.is_empty() {
			println!("No tabs open");
		} else {
			println!("Open tabs:");
			for item in array {
				if let Some(obj) = item.as_object() {
					let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
					let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("unknown");
					let title = obj.get("title").and_then(|v| v.as_str()).unwrap_or("");
					let owner = obj.get("owner").and_then(|v| v.as_str()).unwrap_or("unknown");
					println!("  {} - {} ({}) [owner: {}]", id, url, title, owner);
				}
			}
		}
	} else {
		println!("{}", result);
	}
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
