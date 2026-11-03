use anyhow::{Context, Result};
use ewd_core::events::NetworkEvent;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;

const GENERIC_SEGMENTS: &[&str] = &[
	"api",
	"rest",
	"graphql",
	"v1",
	"v2",
	"v3",
	"_",
	"s3",
	"static",
	"assets",
	"cdn",
	"public",
	"media",
	"files",
	"home",
	"home-static",
	"content",
	"resources",
	"res",
];
const MAX_NAME_SEGMENTS: usize = 3;

const RESERVED: &[&str] = &[
	"default", "function", "class", "return", "new", "delete", "var", "let", "const", "this", "typeof", "void", "in",
	"of", "if", "else", "for", "while", "url", "options", "response", "fetch", "await", "async", "export", "import",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeFormat {
	ES6,
	CommonJS,
	TypeScript,
}

pub fn generate_api_file(events: &[&NetworkEvent], entry_path: &Path) -> Result<PathBuf> {
	if events.is_empty() {
		anyhow::bail!("Нет событий для генерации");
	}

	let extension = entry_path
		.extension()
		.and_then(|e| e.to_str())
		.unwrap_or("js")
		.to_ascii_lowercase();

	let (output_filename, format) = match extension.as_str() {
		"ts" => ("network-api.ts", CodeFormat::TypeScript),
		"cjs" => ("network-api.cjs", CodeFormat::CommonJS),
		_ => ("network-api.js", CodeFormat::ES6),
	};

	let output_dir = entry_path
		.parent()
		.filter(|p| !p.as_os_str().is_empty())
		.unwrap_or_else(|| Path::new("."));
	let output_path = output_dir.join(output_filename);

	let mut function_names: Vec<String> = events
		.iter()
		.map(|e| extract_function_name(&e.method, &e.url))
		.collect();
	deduplicate_names(&mut function_names);

	let mut code = String::new();
	code.push_str("// Auto-generated API file. Do not edit manually.\n");
	code.push_str(&format!("// Source: {} network request(s)\n", events.len()));
	code.push_str("// Regenerated on every export from the Network tab.\n\n");

	for (event, func_name) in events.iter().zip(function_names.iter()) {
		code.push_str(&generate_function(event, func_name, format));
		code.push('\n');
	}

	if format == CodeFormat::CommonJS {
		let exports: Vec<String> = function_names.iter().map(|n| format!("  {}", n)).collect();
		code.push_str(&format!("module.exports = {{\n{}\n}};\n", exports.join(",\n")));
	}

	fs::write(&output_path, code)
		.with_context(|| format!("Не удалось записать файл API: {}", output_path.display()))?;

	Ok(output_path)
}

#[derive(Debug, Default)]
struct UrlParts {
	origin: String,
	segments: Vec<String>,
	query: Vec<(String, String)>,
}

fn parse_url_parts(raw: &str) -> UrlParts {
	if let Ok(u) = Url::parse(raw) {
		let mut origin = String::new();
		if let Some(host) = u.host_str() {
			origin.push_str(u.scheme());
			origin.push_str("://");
			origin.push_str(host);
			if let Some(port) = u.port() {
				origin.push(':');
				origin.push_str(&port.to_string());
			}
		}
		return UrlParts {
			origin,
			segments: split_path(u.path()),
			query: u.query_pairs().map(|(k, v)| (k.into_owned(), v.into_owned())).collect(),
		};
	}

	let (path, query_str) = match raw.split_once('?') {
		Some((p, q)) => (p, Some(q)),
		None => (raw, None),
	};

	let query = query_str
		.map(|q| {
			q.split('&')
				.filter(|pair| !pair.is_empty())
				.map(|pair| match pair.split_once('=') {
					Some((k, v)) => (k.to_string(), v.to_string()),
					None => (pair.to_string(), String::new()),
				})
				.collect()
		})
		.unwrap_or_default();

	UrlParts {
		origin: String::new(),
		segments: split_path(path),
		query,
	}
}

fn split_path(path: &str) -> Vec<String> {
	path.split('/')
		.filter(|s| !s.is_empty())
		.map(|s| s.to_string())
		.collect()
}

fn is_dynamic_segment(s: &str) -> bool {
	if s.is_empty() {
		return false;
	}
	if s.chars().all(|c| c.is_ascii_digit()) {
		return true;
	}
	let dashes = s.chars().filter(|c| *c == '-').count();
	if s.len() == 36 && dashes == 4 {
		return s.chars().filter(|c| *c != '-').all(|c| c.is_ascii_hexdigit());
	}
	s.len() >= 16 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_hash_like(s: &str) -> bool {
	if s.len() <= 12 {
		return false;
	}
	let hex_count = s.chars().filter(|c| c.is_ascii_hexdigit()).count();
	let hex_ratio = hex_count as f64 / s.len() as f64;
	hex_ratio > 0.7
}

fn to_camel_case(s: &str) -> String {
	let mut result = String::new();
	let mut upper_next = true;
	for c in s.chars() {
		if !c.is_alphanumeric() {
			upper_next = true;
			continue;
		}
		if upper_next {
			result.extend(c.to_uppercase());
			upper_next = false;
		} else {
			result.push(c);
		}
	}
	result
}

fn method_prefix(method: &str) -> &'static str {
	match method.to_ascii_uppercase().as_str() {
		"GET" => "fetch",
		"POST" => "post",
		"PUT" => "put",
		"PATCH" => "patch",
		"DELETE" => "delete",
		"HEAD" => "head",
		"OPTIONS" => "options",
		_ => "request",
	}
}

fn strip_file_extension(segment: &str) -> String {
	match segment.rsplit_once('.') {
		Some((name, ext)) if ext.len() <= 4 && ext.chars().all(|c| c.is_ascii_alphanumeric()) => name.to_string(),
		_ => segment.to_string(),
	}
}

fn extract_function_name(method: &str, url: &str) -> String {
	let prefix = method_prefix(method);
	let parts = parse_url_parts(url);

	let meaningful: Vec<String> = parts
		.segments
		.iter()
		.filter(|s| !is_dynamic_segment(s))
		.filter(|s| !GENERIC_SEGMENTS.contains(&s.to_ascii_lowercase().as_str()))
		.filter(|s| !is_hash_like(s))
		.map(|s| strip_file_extension(s))
		.collect();

	if meaningful.is_empty() {
		let fallback = if parts.segments.is_empty() { "Root" } else { "Resource" };
		return format!("{}{}", prefix, fallback);
	}

	let name: String = if meaningful.len() > 2 {
		to_camel_case(&meaningful[meaningful.len() - 1])
	} else {
		let start = meaningful.len().saturating_sub(MAX_NAME_SEGMENTS);
		meaningful[start..].iter().map(|s| to_camel_case(s)).collect()
	};

	format!("{}{}", prefix, name)
}

fn sanitize_identifier(raw: &str) -> String {
	let mut out = String::new();
	for (i, c) in raw.chars().enumerate() {
		if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
			if i == 0 && c.is_ascii_digit() {
				out.push('_');
			}
			out.push(c);
		} else {
			out.push('_');
		}
	}
	if out.is_empty() {
		out.push_str("param");
	}
	if RESERVED.contains(&out.as_str()) {
		out.insert(0, '_');
	}
	out
}

fn unique_name(base: &str, taken: &mut HashSet<String>) -> String {
	let mut candidate = base.to_string();
	let mut counter = 1;
	while taken.contains(&candidate) {
		candidate = format!("{}_{}", base, counter);
		counter += 1;
	}
	taken.insert(candidate.clone());
	candidate
}

fn deduplicate_names(names: &mut [String]) {
	let mut used: HashSet<String> = HashSet::new();
	for name in names.iter_mut() {
		let base = name.clone();
		let mut candidate = base.clone();
		let mut counter = 1;
		while used.contains(&candidate) {
			candidate = format!("{}_{}", base, counter);
			counter += 1;
		}
		used.insert(candidate.clone());
		*name = candidate;
	}
}

#[cfg(test)]
fn extract_path_params(url: &str) -> Vec<(String, String)> {
	let parts = parse_url_parts(url);
	let mut params = Vec::new();
	let mut index = 0usize;
	for segment in &parts.segments {
		if is_dynamic_segment(segment) {
			let name = if index == 0 {
				"id".to_string()
			} else {
				format!("id{}", index)
			};
			params.push((name, segment.clone()));
			index += 1;
		}
	}
	params
}

#[cfg(test)]
fn extract_query_params(url: &str) -> Vec<(String, String)> {
	parse_url_parts(url).query
}

fn js_string(value: &str) -> String {
	let mut out = String::with_capacity(value.len() + 2);
	out.push('\'');
	for c in value.chars() {
		match c {
			'\'' => out.push_str("\\'"),
			'\\' => out.push_str("\\\\"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
			c => out.push(c),
		}
	}
	out.push('\'');
	out
}

fn escape_template(value: &str) -> String {
	value.replace('\\', "\\\\").replace('`', "\\`").replace("${", "\\${")
}

fn is_numeric_value(value: &str) -> bool {
	!value.is_empty()
		&& value.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '.')
		&& value.parse::<f64>().is_ok()
}

fn literal_for(value: &str) -> (&'static str, String) {
	if is_numeric_value(value) {
		("number", value.to_string())
	} else {
		("string", js_string(value))
	}
}

fn format_headers(headers: &[(String, String)]) -> String {
	let lines: Vec<String> = headers
		.iter()
		.filter(|(k, _)| !k.is_empty() && !k.starts_with(':'))
		.map(|(k, v)| format!("    {}: {}", js_string(k), js_string(v)))
		.collect();

	if lines.is_empty() {
		"{}".to_string()
	} else {
		format!("{{\n{}\n  }}", lines.join(",\n"))
	}
}

struct FnParam {
	name: String,
	literal: String,
	ty: &'static str,
}

fn generate_function(event: &NetworkEvent, func_name: &str, format: CodeFormat) -> String {
	let parts = parse_url_parts(&event.url);
	let method = event.method.to_ascii_uppercase();
	let is_read = method == "GET" || method == "HEAD";

	let mut params: Vec<FnParam> = Vec::new();
	let mut taken: HashSet<String> = HashSet::new();
	let mut path_segments: Vec<String> = Vec::new();
	let mut dynamic_index = 0usize;

	for segment in &parts.segments {
		if is_dynamic_segment(segment) {
			let base = if dynamic_index == 0 {
				"id".to_string()
			} else {
				format!("id{}", dynamic_index)
			};
			let name = unique_name(&base, &mut taken);
			dynamic_index += 1;
			let (ty, literal) = literal_for(segment);
			path_segments.push(format!("${{encodeURIComponent({})}}", name));
			params.push(FnParam { name, literal, ty });
		} else {
			path_segments.push(escape_template(segment));
		}
	}

	let mut query_parts: Vec<String> = Vec::new();
	for (key, value) in &parts.query {
		let name = unique_name(&sanitize_identifier(key), &mut taken);
		let (ty, literal) = literal_for(value);
		query_parts.push(format!("{}=${{encodeURIComponent({})}}", escape_template(key), name));
		params.push(FnParam { name, literal, ty });
	}

	let path_str = path_segments.join("/");
	let needs_template = dynamic_index > 0 || !query_parts.is_empty();

	let url_expr = if needs_template {
		let mut s = String::from("`");
		s.push_str(&escape_template(&parts.origin));
		s.push('/');
		s.push_str(&path_str);
		if !query_parts.is_empty() {
			s.push('?');
			s.push_str(&query_parts.join("&"));
		}
		s.push('`');
		s
	} else {
		let mut s = parts.origin.clone();
		if !path_str.is_empty() {
			s.push('/');
			s.push_str(&path_str);
		}
		js_string(&s)
	};

	let args: Vec<String> = params
		.iter()
		.map(|p| match format {
			CodeFormat::TypeScript => format!("{}: {} = {}", p.name, p.ty, p.literal),
			_ => format!("{} = {}", p.name, p.literal),
		})
		.collect();

	let (return_type, return_stmt) = if is_read {
		(
			if format == CodeFormat::TypeScript {
				": Promise<any>"
			} else {
				""
			},
			"  return response.json();",
		)
	} else {
		(
			if format == CodeFormat::TypeScript {
				": Promise<Response>"
			} else {
				""
			},
			"  return response;",
		)
	};

	let export_keyword = match format {
		CodeFormat::CommonJS => "",
		_ => "export ",
	};

	let mut code = String::new();

	code.push_str("/**\n");
	code.push_str(&format!(" * {} {}\n", method, event.url));
	if let Some(status) = event.status_code {
		match event.response_time_ms {
			Some(ms) => code.push_str(&format!(" * Status: {} ({}ms)\n", status, ms)),
			None => code.push_str(&format!(" * Status: {}\n", status)),
		}
	}
	if !event.response_headers.is_empty() {
		code.push_str(" * Response headers:\n");
		for (key, value) in &event.response_headers {
			code.push_str(&format!(" *   {}: {}\n", key, value));
		}
	}
	code.push_str(" */\n");

	code.push_str(&format!(
		"{}async function {}({}){} {{\n",
		export_keyword,
		func_name,
		args.join(", "),
		return_type
	));

	code.push_str(&format!("  const url = {};\n", url_expr));
	code.push_str("  const options = {\n");
	code.push_str(&format!("    method: {},\n", js_string(&method)));
	code.push_str(&format!("    headers: {},\n", format_headers(&event.request_headers)));
	code.push_str("    credentials: 'include',\n");
	if !is_read {
		code.push_str("    body: JSON.stringify({ /* TODO: paste request body here */ }),\n");
	}
	code.push_str("  };\n\n");
	code.push_str("  const response = await fetch(url, options);\n");
	code.push_str(return_stmt);
	code.push_str("\n}\n");

	code
}

#[cfg(test)]
mod tests {
	use super::*;
	use ewd_core::events::NetworkEventType;

	fn make_event(method: &str, url: &str) -> NetworkEvent {
		NetworkEvent {
			url: url.to_string(),
			method: method.to_string(),
			status_code: Some(200),
			response_time_ms: Some(10),
			response_size_bytes: Some(1024),
			event_type: NetworkEventType::Finished,
			request_headers: vec![
				("Content-Type".to_string(), "application/json".to_string()),
				("Authorization".to_string(), "Bearer token".to_string()),
			],
			response_headers: vec![("X-Request-Id".to_string(), "12345".to_string())],
		}
	}

	fn temp_entry(test_name: &str, file: &str) -> PathBuf {
		let dir = std::env::temp_dir().join(format!("ewd-codegen-{}-{}", std::process::id(), test_name));
		fs::create_dir_all(&dir).unwrap();
		let entry = dir.join(file);
		fs::write(&entry, "// test").unwrap();
		entry
	}

	fn cleanup(entry: &Path) {
		if let Some(dir) = entry.parent() {
			fs::remove_dir_all(dir).ok();
		}
	}

	#[test]
	fn test_to_camel_case() {
		assert_eq!(to_camel_case("portal-feed"), "PortalFeed");
		assert_eq!(to_camel_case("user_profile"), "UserProfile");
		assert_eq!(to_camel_case("simple"), "Simple");
		assert_eq!(to_camel_case("multi-word-name"), "MultiWordName");
	}

	#[test]
	fn test_extract_function_name() {
		assert_eq!(
			extract_function_name("GET", "https://example.com/portal/feed"),
			"fetchPortalFeed"
		);
		assert_eq!(
			extract_function_name("POST", "https://example.com/users/123"),
			"postUsers"
		);
		assert_eq!(
			extract_function_name("DELETE", "https://example.com/api/items/456"),
			"deleteItems"
		);
		assert_eq!(
			extract_function_name("GET", "https://example.com/api/v1/users"),
			"fetchUsers"
		);
		assert_eq!(extract_function_name("GET", "https://example.com/"), "fetchRoot");
	}

	#[test]
	fn test_extract_function_name_ignores_hash() {
		assert_eq!(
			extract_function_name(
				"GET",
				"https://example.com/s3/home-static/5be261058ebed63a30849c5d966ab70f/script.js"
			),
			"fetchScript"
		);
	}

	#[test]
	fn test_extract_path_params() {
		let params = extract_path_params("https://example.com/users/123/posts/456");
		assert_eq!(params.len(), 2);
		assert_eq!(params[0], ("id".to_string(), "123".to_string()));
		assert_eq!(params[1], ("id1".to_string(), "456".to_string()));
	}

	#[test]
	fn test_extract_query_params() {
		let params = extract_query_params("https://example.com/api?limit=10&offset=20");
		assert_eq!(params.len(), 2);
		assert_eq!(params[0], ("limit".to_string(), "10".to_string()));
		assert_eq!(params[1], ("offset".to_string(), "20".to_string()));
	}

	#[test]
	fn test_deduplicate_names() {
		let mut names = vec![
			"fetchUsers".to_string(),
			"fetchUsers".to_string(),
			"fetchPosts".to_string(),
			"fetchUsers".to_string(),
		];
		deduplicate_names(&mut names);
		assert_eq!(names[0], "fetchUsers");
		assert_eq!(names[1], "fetchUsers_1");
		assert_eq!(names[2], "fetchPosts");
		assert_eq!(names[3], "fetchUsers_2");
	}

	#[test]
	fn test_deduplicate_names_no_collision() {
		let mut names = vec![
			"postClick".to_string(),
			"postClick".to_string(),
			"postClick_1".to_string(),
		];
		deduplicate_names(&mut names);
		assert_eq!(names, vec!["postClick", "postClick_1", "postClick_1_1"]);
	}

	#[test]
	fn test_generate_function_template_url() {
		let event = make_event("GET", "https://example.com/users/123/posts/456?limit=10");
		let code = generate_function(&event, "fetchUsersPosts", CodeFormat::ES6);

		assert!(code.contains("async function fetchUsersPosts(id = 123, id1 = 456, limit = 10)"));
		assert!(code.contains(
            "const url = `https://example.com/users/${encodeURIComponent(id)}/posts/${encodeURIComponent(id1)}?limit=${encodeURIComponent(limit)}`;"
        ));
		assert!(code.contains("return response.json();"));
	}

	#[test]
	fn test_generate_function_sanitizes_query_names() {
		let event = make_event("GET", "https://example.com/api?filter[x]=1&a-b=2");
		let code = generate_function(&event, "fetchRoot", CodeFormat::ES6);
		assert!(code.contains("filter_x_ = 1"));
		assert!(code.contains("a_b = 2"));
	}

	#[test]
	fn test_generate_function_escapes_headers() {
		let mut event = make_event("GET", "https://example.com/api/data");
		event.request_headers = vec![("X-Quote".to_string(), "it's".to_string())];
		let code = generate_function(&event, "fetchData", CodeFormat::ES6);
		assert!(code.contains("'X-Quote': 'it\\'s'"));
	}

	#[test]
	fn test_generate_function_relative_url_does_not_panic() {
		let event = make_event("GET", "/api/users?limit=5");
		let code = generate_function(&event, "fetchUsers", CodeFormat::ES6);
		assert!(code.contains("const url = `/api/users?limit=${encodeURIComponent(limit)}`;"));
	}

	#[test]
	fn test_generate_api_file_es6() {
		let events = vec![
			make_event("GET", "https://example.com/api/users"),
			make_event("POST", "https://example.com/api/posts"),
		];
		let refs: Vec<&NetworkEvent> = events.iter().collect();
		let entry = temp_entry("es6", "entry.js");

		let output = generate_api_file(&refs, &entry).unwrap();
		assert_eq!(output.file_name().unwrap(), "network-api.js");

		let content = fs::read_to_string(&output).unwrap();
		assert!(content.contains("export async function fetchUsers"));
		assert!(content.contains("export async function postPosts"));
		assert!(content.contains("Response headers:"));
		assert!(content.contains("X-Request-Id: 12345"));
		assert!(!content.contains("module.exports"));

		cleanup(&entry);
	}

	#[test]
	fn test_generate_api_file_commonjs() {
		let events = vec![make_event("GET", "https://example.com/api/data")];
		let refs: Vec<&NetworkEvent> = events.iter().collect();
		let entry = temp_entry("cjs", "entry.cjs");

		let output = generate_api_file(&refs, &entry).unwrap();
		assert_eq!(output.file_name().unwrap(), "network-api.cjs");

		let content = fs::read_to_string(&output).unwrap();
		assert!(content.contains("async function fetchData"));
		assert!(!content.contains("export async function"));
		assert!(content.contains("module.exports = {\n  fetchData\n};"));

		cleanup(&entry);
	}

	#[test]
	fn test_generate_api_file_typescript_get() {
		let events = vec![make_event("GET", "https://example.com/api/items?limit=10")];
		let refs: Vec<&NetworkEvent> = events.iter().collect();
		let entry = temp_entry("ts_get", "entry.ts");

		let output = generate_api_file(&refs, &entry).unwrap();
		assert_eq!(output.file_name().unwrap(), "network-api.ts");

		let content = fs::read_to_string(&output).unwrap();
		assert!(content.contains("export async function fetchItems(limit: number = 10): Promise<any>"));
		assert!(content.contains("return response.json();"));

		cleanup(&entry);
	}

	#[test]
	fn test_generate_api_file_typescript_post() {
		let events = vec![make_event("POST", "https://example.com/api/items")];
		let refs: Vec<&NetworkEvent> = events.iter().collect();
		let entry = temp_entry("ts_post", "entry.ts");

		let output = generate_api_file(&refs, &entry).unwrap();
		let content = fs::read_to_string(&output).unwrap();
		assert!(content.contains("export async function postItems(): Promise<Response>"));
		assert!(content.contains("body: JSON.stringify({ /* TODO: paste request body here */ }),"));
		assert!(content.contains("return response;"));

		cleanup(&entry);
	}

	#[test]
	fn test_generate_api_file_overwrites() {
		let first = vec![make_event("GET", "https://example.com/api/first")];
		let refs: Vec<&NetworkEvent> = first.iter().collect();
		let entry = temp_entry("overwrite", "entry.js");

		let output = generate_api_file(&refs, &entry).unwrap();
		assert!(fs::read_to_string(&output).unwrap().contains("fetchFirst"));

		let second = vec![make_event("GET", "https://example.com/api/second")];
		let refs2: Vec<&NetworkEvent> = second.iter().collect();
		let output2 = generate_api_file(&refs2, &entry).unwrap();

		let content = fs::read_to_string(&output2).unwrap();
		assert!(content.contains("fetchSecond"));
		assert!(!content.contains("fetchFirst"));

		cleanup(&entry);
	}

	#[test]
	fn test_generate_api_file_empty_fails() {
		let entry = temp_entry("empty", "entry.js");
		let result = generate_api_file(&[], &entry);
		assert!(result.is_err());
		cleanup(&entry);
	}
}
