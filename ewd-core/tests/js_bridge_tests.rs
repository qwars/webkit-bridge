// ewd-core/tests/js_bridge_tests.rs

use ewd_core::tab::parse_js_message;
use serde_json::json;

// ========== Базовые сценарии ==========

#[test]
fn test_parse_console_log() {
	let raw = r#"{"type":"console_log","data":{"text":"Hello world"}}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "console_log");
	assert_eq!(data["text"], "Hello world");
}

#[test]
fn test_parse_console_warn() {
	let raw = r#"{"type":"console_warn","data":{"text":"Warning!"}}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "console_warn");
	assert_eq!(data["text"], "Warning!");
}

#[test]
fn test_parse_console_error() {
	let raw = r#"{"type":"console_error","data":{"text":"Error occurred"}}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "console_error");
	assert_eq!(data["text"], "Error occurred");
}

#[test]
fn test_parse_console_dir() {
	let raw = r#"{"type":"console_dir","data":{"text":"{\"key\":\"value\"}"}}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "console_dir");
	assert!(data["text"].as_str().unwrap().contains("key"));
}

#[test]
fn test_parse_custom_event() {
	let raw = r#"{"type":"custom_event","data":{"foo":"bar","num":42}}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "custom_event");
	assert_eq!(data["foo"], "bar");
	assert_eq!(data["num"], 42);
}

// ========== Граничные случаи ==========

#[test]
fn test_parse_empty_object() {
	let raw = r#"{}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "unknown");
	assert_eq!(data, serde_json::Value::Null);
}

#[test]
fn test_parse_missing_type() {
	let raw = r#"{"data":{"foo":"bar"}}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "unknown");
	assert_eq!(data["foo"], "bar");
}

#[test]
fn test_parse_missing_data() {
	let raw = r#"{"type":"test_event"}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "test_event");
	assert_eq!(data, serde_json::Value::Null);
}

#[test]
fn test_parse_null_type() {
	let raw = r#"{"type":null,"data":{}}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "unknown");
	assert_eq!(data, json!({}));
}

#[test]
fn test_parse_numeric_type() {
	// type не является строкой → должно стать "unknown"
	let raw = r#"{"type":123,"data":{}}"#;
	let (msg_type, _) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "unknown");
}

#[test]
fn test_parse_boolean_type() {
	let raw = r#"{"type":true,"data":{}}"#;
	let (msg_type, _) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "unknown");
}

// ========== Ошибки парсинга ==========

#[test]
fn test_parse_invalid_json() {
	let raw = r#"{"type":"test","data":invalid}"#;
	let err = parse_js_message(raw).unwrap_err();
	assert!(err.contains("JSON parse error"));
}

#[test]
fn test_parse_empty_string() {
	let err = parse_js_message("").unwrap_err();
	assert!(err.contains("JSON parse error"));
}

#[test]
fn test_parse_not_json() {
	let raw = "just a plain string";
	let err = parse_js_message(raw).unwrap_err();
	assert!(err.contains("JSON parse error"));
}

#[test]
fn test_parse_trailing_comma() {
	let raw = r#"{"type":"test","data":{}}"#;
	// JSON не поддерживает trailing comma — должно упасть
	let raw_with_comma = r#"{"type":"test","data":{},}"#;
	assert!(parse_js_message(raw_with_comma).is_err());
	// А валидный вариант должен работать
	assert!(parse_js_message(raw).is_ok());
}

// ========== Unicode и спецсимволы ==========

#[test]
fn test_parse_unicode_text() {
	let raw = r#"{"type":"console_log","data":{"text":"Привет мир 🌍"}}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "console_log");
	assert_eq!(data["text"], "Привет мир 🌍");
}

#[test]
fn test_parse_escaped_characters() {
	let raw = r#"{"type":"console_log","data":{"text":"line1\nline2\ttab\"quote\\backslash"}}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "console_log");
	let text = data["text"].as_str().unwrap();
	assert!(text.contains('\n'));
	assert!(text.contains('\t'));
	assert!(text.contains('"'));
	assert!(text.contains('\\'));
}

#[test]
fn test_parse_chinese_characters() {
	let raw = r#"{"type":"console_log","data":{"text":"你好世界"}}"#;
	let (msg_type, data) = parse_js_message(raw).unwrap();
	assert_eq!(msg_type, "console_log");
	assert_eq!(data["text"], "你好世界");
}

#[test]
fn test_parse_emoji_only() {
	let raw = r#"{"type":"console_log","data":{"text":"🚀🎉✅"}}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data["text"], "🚀🎉✅");
}

// ========== Вложенные структуры ==========

#[test]
fn test_parse_nested_object() {
	let raw = r#"{"type":"test","data":{"user":{"name":"Alice","age":30}}}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data["user"]["name"], "Alice");
	assert_eq!(data["user"]["age"], 30);
}

#[test]
fn test_parse_array_in_data() {
	let raw = r#"{"type":"test","data":{"items":[1,2,3]}}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data["items"], json!([1, 2, 3]));
}

#[test]
fn test_parse_deeply_nested() {
	let raw = r#"{"type":"test","data":{"a":{"b":{"c":{"d":"deep"}}}}}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data["a"]["b"]["c"]["d"], "deep");
}

#[test]
fn test_parse_array_of_objects() {
	let raw = r#"{"type":"test","data":{"users":[{"id":1},{"id":2}]}}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data["users"][0]["id"], 1);
	assert_eq!(data["users"][1]["id"], 2);
}

// ========== Числовые типы ==========

#[test]
fn test_parse_integer_data() {
	let raw = r#"{"type":"test","data":42}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data, 42);
}

#[test]
fn test_parse_float_data() {
	let raw = r#"{"type":"test","data":1.23456}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data.as_f64(), Some(1.23456));
}

#[test]
fn test_parse_negative_number() {
	let raw = r#"{"type":"test","data":-100}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data, -100);
}

#[test]
fn test_parse_boolean_data() {
	let raw = r#"{"type":"test","data":true}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data, true);
}

#[test]
fn test_parse_null_data() {
	let raw = r#"{"type":"test","data":null}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data, serde_json::Value::Null);
}

// ========== Пустые и длинные строки ==========

#[test]
fn test_parse_empty_text() {
	let raw = r#"{"type":"console_log","data":{"text":""}}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data["text"], "");
}

#[test]
fn test_parse_long_text() {
	let long_text = "x".repeat(10_000);
	let raw = format!(r#"{{"type":"console_log","data":{{"text":"{}"}}}}"#, long_text);
	let (_, data) = parse_js_message(&raw).unwrap();
	assert_eq!(data["text"].as_str().unwrap().len(), 10_000);
}

#[test]
fn test_parse_whitespace_only_text() {
	let raw = r#"{"type":"console_log","data":{"text":"   \t\n  "}}"#;
	let (_, data) = parse_js_message(raw).unwrap();
	assert_eq!(data["text"].as_str().unwrap().trim(), "");
}
