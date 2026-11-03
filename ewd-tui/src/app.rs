use crate::build::BuildOutput;
use crate::create_project::{CreateProjectField, CreateProjectState};
use crate::input_field::InputField;
use crate::ipc::IpcClient;
use crate::keys::Keymap;
use crate::projects::{create_project, list_projects, Project};
use crate::text_buffer::TextBuffer;
use crate::watcher::{BuildEvent, ProjectWatcher};
use anyhow::{Context, Result};
use ewd_core::events::NetworkEvent;
use ratatui::style::{Color, Style};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;

const MAX_LOG_LINES: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
	Projects,
	Project(String),
	CreateProject,
	ConfirmDelete,
	NetworkInspect(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectStatus {
	Stopped,
	Running,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferKind {
	Console,
	Logs,
	Network,
}

#[derive(Debug, Clone)]
pub struct ProjectState {
	pub project: Project,
	pub status: ProjectStatus,
	pub tab_id: Option<String>,
	pub mtime: SystemTime,
	pub tab_visible: bool,
	pub last_build_success: Option<bool>,
}

#[derive(Debug)]
pub struct BuildResult {
	pub dir: String,
	pub output: Result<BuildOutput>,
	pub build_id: u64,
}

pub struct AppState {
	pub screen: Screen,
	pub projects: Vec<ProjectState>,
	pub selected_index: usize,
	pub search_query: String,
	pub search_mode: bool,
	pub keymap: Keymap,
	pub should_quit: bool,
	pub ipc_client: Option<Arc<IpcClient>>,
	pub log_rx: Option<mpsc::UnboundedReceiver<String>>,
	pub log_buffer: TextBuffer,
	pub console_buffer: TextBuffer,
	pub network_rx: Option<mpsc::UnboundedReceiver<Value>>,
	pub network_buffer: TextBuffer,
	pub network_events: Vec<NetworkEvent>,
	pub network_search_query: String,
	pub network_search_mode: bool,
	pub network_cursor: usize,
	pub active_buffer: BufferKind,
	pub create_project_state: CreateProjectState,
	pub clipboard: Option<arboard::Clipboard>,
	pub watcher_tx: mpsc::UnboundedSender<BuildEvent>,
	pub watcher_rx: Option<mpsc::UnboundedReceiver<BuildEvent>>,
	pub active_watchers: HashMap<String, ProjectWatcher>,
	pub confirm_delete: Option<String>,
	pub is_building: bool,
	pub pending_start: Option<String>,
	pub(crate) build_rx: Option<mpsc::UnboundedReceiver<BuildResult>>,
	pub(crate) build_tx: mpsc::UnboundedSender<BuildResult>,
	pub(crate) active_builds: HashMap<String, tokio::task::JoinHandle<()>>,
	pub(crate) next_build_id: u64,
	pub(crate) active_build_id: HashMap<String, u64>,
	pub marked_network_indices: HashSet<usize>,
}

impl AppState {
	pub async fn new(
		log_rx: mpsc::UnboundedReceiver<String>,
		network_rx: mpsc::UnboundedReceiver<Value>,
	) -> Result<Self> {
		let mut projects = Vec::new();
		for (project, mtime) in list_projects()? {
			projects.push(ProjectState {
				project,
				status: ProjectStatus::Stopped,
				tab_id: None,
				mtime,
				tab_visible: false,
				last_build_success: None,
			});
		}

		let clipboard = arboard::Clipboard::new().ok();
		let (watcher_tx, watcher_rx) = mpsc::unbounded_channel();
		let (build_tx, build_rx) = mpsc::unbounded_channel();

		Ok(Self {
			screen: Screen::Projects,
			projects,
			selected_index: 0,
			keymap: Keymap::default(),
			should_quit: false,
			ipc_client: None,
			log_rx: Some(log_rx),
			log_buffer: TextBuffer::new_with_limit(MAX_LOG_LINES),
			console_buffer: TextBuffer::new_with_limit(MAX_LOG_LINES),
			network_rx: Some(network_rx),
			network_buffer: TextBuffer::new_with_limit(MAX_LOG_LINES),
			network_events: Vec::new(),
			network_search_query: String::new(),
			network_search_mode: false,
			network_cursor: 0,
			active_buffer: BufferKind::Logs,
			create_project_state: CreateProjectState::default(),
			clipboard,
			watcher_tx,
			watcher_rx: Some(watcher_rx),
			active_watchers: HashMap::new(),
			search_query: String::new(),
			search_mode: false,
			confirm_delete: None,
			is_building: false,
			pending_start: None,
			build_rx: Some(build_rx),
			build_tx,
			active_builds: HashMap::new(),
			next_build_id: 0,
			active_build_id: HashMap::new(),
			marked_network_indices: HashSet::new(),
		})
	}

	pub fn set_ipc_client(&mut self, client: Arc<IpcClient>) {
		self.ipc_client = Some(client);
	}

	pub fn current_project(&self) -> Option<&ProjectState> {
		match &self.screen {
			Screen::Project(dir) => self.projects.iter().find(|p| p.project.dir == *dir),
			_ => None,
		}
	}

	pub fn drain_logs(&mut self) {
		if let Some(rx) = &mut self.log_rx {
			while let Ok(line) = rx.try_recv() {
				self.log_buffer.push_line(line);
			}
		}
	}

	pub fn drain_network_events(&mut self) {
		let rx = match self.network_rx.as_mut() {
			Some(rx) => rx,
			None => return,
		};

		loop {
			match rx.try_recv() {
				Ok(raw) => {
					let method = match raw.get("method").and_then(|m| m.as_str()) {
						Some(m) => m,
						None => continue,
					};

					if method != "network_event" {
						continue;
					}

					let params = match raw.get("params") {
						Some(p) => p,
						None => continue,
					};

					let event_obj = match params.get("event") {
						Some(e) => e,
						None => continue,
					};

					let event: NetworkEvent = match serde_json::from_value(event_obj.clone()) {
						Ok(e) => e,
						Err(e) => {
							log::warn!("Failed to parse NetworkEvent: {}", e);
							continue;
						}
					};

					let status = event
						.status_code
						.map(|s| s.to_string())
						.unwrap_or_else(|| "-".to_string());
					let time = event
						.response_time_ms
						.map(|t| format!("{}ms", t))
						.unwrap_or_else(|| "-".to_string());
					let event_type_str = match event.event_type {
						ewd_core::events::NetworkEventType::Started => "started",
						ewd_core::events::NetworkEventType::Finished => "finished",
						ewd_core::events::NetworkEventType::Failed => "failed",
					};

					let color = match event.event_type {
						ewd_core::events::NetworkEventType::Started => Color::DarkGray,
						ewd_core::events::NetworkEventType::Finished => {
							if event.status_code == Some(200) {
								Color::Green
							} else {
								Color::Yellow
							}
						}
						ewd_core::events::NetworkEventType::Failed => Color::Red,
					};

					let line = format!(
						"[{}] {} {} {} {}",
						time, event.method, status, event.url, event_type_str
					);

					self.network_buffer.push_line_styled(line, Style::default().fg(color));
					self.network_events.push(event);
				}
				Err(mpsc::error::TryRecvError::Empty) => break,
				Err(mpsc::error::TryRecvError::Disconnected) => break,
			}
		}

		let overflow = self.network_events.len().saturating_sub(MAX_LOG_LINES);
		if overflow > 0 {
			self.trim_network_history(overflow);
		}
	}

	pub fn trim_network_history(&mut self, trimmed: usize) {
		if trimmed == 0 {
			return;
		}

		let removed = trimmed.min(self.network_events.len());
		if removed > 0 {
			self.network_events.drain(0..removed);
		}

		let shifted: HashSet<usize> = self
			.marked_network_indices
			.iter()
			.filter(|&&idx| idx >= removed)
			.map(|&idx| idx - removed)
			.collect();
		self.marked_network_indices = shifted;

		let len = self.visible_network_indices().len();
		self.network_cursor = if len == 0 { 0 } else { self.network_cursor.min(len - 1) };
	}

	pub fn visible_network_indices(&self) -> Vec<usize> {
		let completed: HashSet<(String, String)> = self
			.network_events
			.iter()
			.filter(|e| {
				matches!(
					e.event_type,
					ewd_core::events::NetworkEventType::Finished | ewd_core::events::NetworkEventType::Failed
				)
			})
			.map(|e| (e.url.clone(), e.method.clone()))
			.collect();

		let mut indices: Vec<usize> = self
			.network_events
			.iter()
			.enumerate()
			.filter(|(_, e)| {
				if matches!(e.event_type, ewd_core::events::NetworkEventType::Started) {
					!completed.contains(&(e.url.clone(), e.method.clone()))
				} else {
					true
				}
			})
			.map(|(i, _)| i)
			.collect();

		if !self.network_search_query.is_empty() {
			let query = self.network_search_query.to_lowercase();
			indices.retain(|&i| {
				let e = &self.network_events[i];
				e.url.to_lowercase().contains(&query) || e.method.to_lowercase().contains(&query)
			});
		}

		indices
	}

	pub fn enter_network_search(&mut self) {
		self.network_search_mode = true;
		self.network_search_query.clear();
		self.network_cursor = 0;
	}

	pub fn exit_network_search(&mut self) {
		self.network_search_mode = false;
		self.network_search_query.clear();
		self.network_cursor = 0;
	}

	pub fn network_search_input_char(&mut self, c: char) {
		self.network_search_query.push(c);
		self.network_cursor = 0;
	}

	pub fn network_search_backspace(&mut self) {
		self.network_search_query.pop();
		self.network_cursor = 0;
	}

	pub fn network_move_up(&mut self) {
		if self.network_cursor > 0 {
			self.network_cursor -= 1;
		}
	}

	pub fn network_move_down(&mut self) {
		let len = self.visible_network_indices().len();
		if len > 0 && self.network_cursor < len.saturating_sub(1) {
			self.network_cursor += 1;
		}
	}

	pub fn network_page_up(&mut self, page_size: usize) {
		self.network_cursor = self.network_cursor.saturating_sub(page_size);
	}

	pub fn network_page_down(&mut self, page_size: usize) {
		let len = self.visible_network_indices().len();
		if len > 0 {
			self.network_cursor = (self.network_cursor + page_size).min(len.saturating_sub(1));
		}
	}

	pub fn network_home(&mut self) {
		self.network_cursor = 0;
	}

	pub fn network_end(&mut self) {
		let len = self.visible_network_indices().len();
		if len > 0 {
			self.network_cursor = len.saturating_sub(1);
		}
	}

	pub fn open_network_inspect(&mut self) {
		let indices = self.visible_network_indices();
		if indices.is_empty() {
			return;
		}
		let cursor = self.network_cursor.min(indices.len().saturating_sub(1));
		let event_idx = indices[cursor];
		self.screen = Screen::NetworkInspect(event_idx);
	}

	pub fn toggle_network_mark(&mut self) {
		let indices = self.visible_network_indices();
		if indices.is_empty() {
			return;
		}
		let cursor = self.network_cursor.min(indices.len().saturating_sub(1));
		let event_idx = indices[cursor];

		if self.marked_network_indices.contains(&event_idx) {
			self.marked_network_indices.remove(&event_idx);
		} else {
			self.marked_network_indices.insert(event_idx);
		}
	}

	pub fn clear_network_marks(&mut self) {
		self.marked_network_indices.clear();
	}

	pub fn marked_events(&self) -> Vec<&NetworkEvent> {
		let mut indices: Vec<usize> = self.marked_network_indices.iter().copied().collect();
		indices.sort();
		indices
			.into_iter()
			.filter_map(|idx| self.network_events.get(idx))
			.collect()
	}

	pub fn start_create_project(&mut self) {
		self.create_project_state = CreateProjectState::default();
		self.screen = Screen::CreateProject;
	}

	pub fn cancel_create_project(&mut self) {
		self.screen = Screen::Projects;
	}

	fn get_focused_field_mut(&mut self) -> &mut InputField {
		match self.create_project_state.focused_field {
			CreateProjectField::Name => &mut self.create_project_state.name,
			CreateProjectField::Dir => &mut self.create_project_state.dir,
			CreateProjectField::Url => &mut self.create_project_state.url,
			CreateProjectField::Entry => &mut self.create_project_state.entry,
		}
	}

	fn clear_field_error(&mut self) {
		let focused = self.create_project_state.focused_field;
		self.create_project_state.invalid_fields.retain(|&f| f != focused);
		self.create_project_state.error = None;
	}

	pub fn input_char(&mut self, c: char) {
		self.clear_field_error();
		self.get_focused_field_mut().insert(c);
	}

	pub fn backspace(&mut self) {
		self.clear_field_error();
		self.get_focused_field_mut().backspace();
	}

	pub fn delete(&mut self) {
		self.clear_field_error();
		self.get_focused_field_mut().delete();
	}

	pub fn move_cursor_left(&mut self) {
		self.get_focused_field_mut().move_left();
	}

	pub fn move_cursor_right(&mut self) {
		self.get_focused_field_mut().move_right();
	}

	pub fn move_to_line_start(&mut self) {
		self.get_focused_field_mut().move_to_line_start();
	}

	pub fn move_to_line_end(&mut self) {
		self.get_focused_field_mut().move_to_line_end();
	}

	pub fn move_word_left(&mut self) {
		self.get_focused_field_mut().move_word_left();
	}

	pub fn move_word_right(&mut self) {
		self.get_focused_field_mut().move_word_right();
	}

	pub fn select_left(&mut self) {
		self.get_focused_field_mut().select_left();
	}

	pub fn select_right(&mut self) {
		self.get_focused_field_mut().select_right();
	}

	pub fn select_to_line_start(&mut self) {
		self.get_focused_field_mut().select_to_line_start();
	}

	pub fn select_to_line_end(&mut self) {
		self.get_focused_field_mut().select_to_line_end();
	}

	pub fn select_word_left(&mut self) {
		self.get_focused_field_mut().select_word_left();
	}

	pub fn select_word_right(&mut self) {
		self.get_focused_field_mut().select_word_right();
	}

	pub fn select_all(&mut self) {
		self.get_focused_field_mut().select_all();
	}

	pub fn copy(&mut self) {
		let text = self.get_focused_field_mut().copy();
		if let Some(clipboard) = &mut self.clipboard {
			let _ = clipboard.set_text(text);
		}
	}

	pub fn cut(&mut self) {
		self.clear_field_error();
		let text = self.get_focused_field_mut().cut();
		if let Some(clipboard) = &mut self.clipboard {
			let _ = clipboard.set_text(text);
		}
	}

	pub fn paste(&mut self) {
		if let Some(clipboard) = &mut self.clipboard {
			if let Ok(text) = clipboard.get_text() {
				self.clear_field_error();
				let is_url = matches!(self.create_project_state.focused_field, CreateProjectField::Url);
				self.get_focused_field_mut().paste(&text, is_url);
			}
		}
	}

	pub fn cut_word(&mut self) {
		self.clear_field_error();
		let text = self.get_focused_field_mut().cut_word();
		if !text.is_empty() {
			if let Some(clipboard) = &mut self.clipboard {
				let _ = clipboard.set_text(text);
			}
		}
	}

	pub fn kill_line(&mut self) {
		self.clear_field_error();
		let text = self.get_focused_field_mut().kill_line();
		if !text.is_empty() {
			if let Some(clipboard) = &mut self.clipboard {
				let _ = clipboard.set_text(text);
			}
		}
	}

	pub fn backward_kill_line(&mut self) {
		self.clear_field_error();
		let text = self.get_focused_field_mut().backward_kill_line();
		if !text.is_empty() {
			if let Some(clipboard) = &mut self.clipboard {
				let _ = clipboard.set_text(text);
			}
		}
	}

	pub fn next_field(&mut self) {
		self.create_project_state.focused_field = match self.create_project_state.focused_field {
			CreateProjectField::Name => CreateProjectField::Dir,
			CreateProjectField::Dir => CreateProjectField::Url,
			CreateProjectField::Url => CreateProjectField::Entry,
			CreateProjectField::Entry => CreateProjectField::Name,
		};
	}

	pub async fn submit_create_project(&mut self) -> Result<bool> {
		self.create_project_state.error = None;
		self.create_project_state.invalid_fields.clear();

		let mut has_errors = false;

		if self.create_project_state.name.text.is_empty() {
			self.create_project_state.invalid_fields.push(CreateProjectField::Name);
			has_errors = true;
		}
		if self.create_project_state.dir.text.is_empty() {
			self.create_project_state.invalid_fields.push(CreateProjectField::Dir);
			has_errors = true;
		}
		if self.create_project_state.url.text.is_empty() {
			self.create_project_state.invalid_fields.push(CreateProjectField::Url);
			has_errors = true;
		}
		if self.create_project_state.entry.text.is_empty() {
			self.create_project_state.invalid_fields.push(CreateProjectField::Entry);
			has_errors = true;
		}

		if has_errors {
			self.create_project_state.error = Some("Заполните все обязательные поля".to_string());
			return Ok(false);
		}

		match create_project(
			&self.create_project_state.name.text,
			&self.create_project_state.dir.text,
			&self.create_project_state.url.text,
			&self.create_project_state.entry.text,
		) {
			Ok(project) => {
				let dir = project.dir.clone();
				self.projects.push(ProjectState {
					project,
					status: ProjectStatus::Stopped,
					tab_id: None,
					mtime: SystemTime::now(),
					tab_visible: false,
					last_build_success: None,
				});
				self.projects
					.sort_by(|a, b| a.project.config.name.cmp(&b.project.config.name));
				self.search_mode = false;
				self.search_query.clear();
				self.selected_index = self.projects.iter().position(|p| p.project.dir == dir).unwrap_or(0);
				self.screen = Screen::Projects;
				Ok(true)
			}
			Err(e) => {
				let err_str = e.to_string();
				let lower = err_str.to_lowercase();
				if lower.contains("каталог") {
					self.create_project_state.invalid_fields.push(CreateProjectField::Dir);
				} else if lower.contains("url") {
					self.create_project_state.invalid_fields.push(CreateProjectField::Url);
				} else if lower.contains("название") {
					self.create_project_state.invalid_fields.push(CreateProjectField::Name);
				} else if lower.contains("entry") || lower.contains("расширен") {
					self.create_project_state.invalid_fields.push(CreateProjectField::Entry);
				}
				self.create_project_state.error = Some(err_str);
				Ok(false)
			}
		}
	}

	pub fn toggle_keymap(&mut self) -> Result<()> {
		use crate::config::{save, GlobalConfig};
		self.keymap = match self.keymap {
			crate::keys::Keymap::Emacs => crate::keys::Keymap::Vim,
			crate::keys::Keymap::Vim => crate::keys::Keymap::Vscode,
			crate::keys::Keymap::Vscode => crate::keys::Keymap::Emacs,
		};
		let config = GlobalConfig { keymap: self.keymap };
		save(&config).context("Failed to write config")?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::projects::ProjectConfig;

	fn mock_project_state(name: &str, dir: &str) -> ProjectState {
		ProjectState {
			project: Project {
				dir: dir.to_string(),
				config: ProjectConfig {
					name: name.to_string(),
					url: "https://example.com".to_string(),
					entry: "main.js".to_string(),
					watch_extensions: vec!["js".into()],
				},
			},
			status: ProjectStatus::Stopped,
			tab_id: None,
			mtime: SystemTime::now(),
			tab_visible: false,
			last_build_success: None,
		}
	}

	fn make_network_event(url: &str, method: &str, event_type: ewd_core::events::NetworkEventType) -> NetworkEvent {
		NetworkEvent {
			url: url.to_string(),
			method: method.to_string(),
			status_code: Some(200),
			response_time_ms: Some(10),
			response_size_bytes: Some(1024),
			event_type,
			request_headers: vec![],
			response_headers: vec![],
		}
	}

	fn mock_state() -> AppState {
		let (watcher_tx, watcher_rx) = mpsc::unbounded_channel();
		let (build_tx, build_rx) = mpsc::unbounded_channel();
		let (_network_tx, network_rx) = mpsc::unbounded_channel();

		AppState {
			screen: Screen::Projects,
			projects: vec![
				mock_project_state("Project A", "proj-a"),
				mock_project_state("Project B", "proj-b"),
			],
			selected_index: 0,
			search_query: String::new(),
			search_mode: false,
			keymap: Keymap::default(),
			should_quit: false,
			ipc_client: None,
			log_rx: None,
			log_buffer: TextBuffer::new_with_limit(MAX_LOG_LINES),
			console_buffer: TextBuffer::new_with_limit(MAX_LOG_LINES),
			network_rx: Some(network_rx),
			network_buffer: TextBuffer::new_with_limit(MAX_LOG_LINES),
			network_events: Vec::new(),
			network_search_query: String::new(),
			network_search_mode: false,
			network_cursor: 0,
			active_buffer: BufferKind::Logs,
			create_project_state: CreateProjectState::default(),
			clipboard: None,
			watcher_tx,
			watcher_rx: Some(watcher_rx),
			active_watchers: HashMap::new(),
			confirm_delete: None,
			is_building: false,
			pending_start: None,
			build_rx: Some(build_rx),
			build_tx,
			active_builds: HashMap::new(),
			next_build_id: 0,
			active_build_id: HashMap::new(),
			marked_network_indices: HashSet::new(),
		}
	}

	#[test]
	fn test_network_cursor_navigation() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://c.com", "GET", ewd_core::events::NetworkEventType::Finished),
		];

		assert_eq!(state.network_cursor, 0);
		state.network_move_down();
		assert_eq!(state.network_cursor, 1);
		state.network_move_down();
		assert_eq!(state.network_cursor, 2);
		state.network_move_down();
		assert_eq!(state.network_cursor, 2);
		state.network_move_up();
		assert_eq!(state.network_cursor, 1);
		state.network_home();
		assert_eq!(state.network_cursor, 0);
		state.network_end();
		assert_eq!(state.network_cursor, 2);
	}

	#[test]
	fn test_network_cursor_respects_filter() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com/api", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://c.com/api", "GET", ewd_core::events::NetworkEventType::Finished),
		];

		state.network_search_query = "api".to_string();
		assert_eq!(state.visible_network_indices(), vec![0, 2]);

		state.network_move_down();
		assert_eq!(state.network_cursor, 1);
		state.network_move_down();
		assert_eq!(state.network_cursor, 1);
	}

	#[test]
	fn test_open_network_inspect() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Finished),
		];
		state.screen = Screen::Project("proj-a".to_string());

		state.network_move_down();
		state.open_network_inspect();

		assert!(matches!(state.screen, Screen::NetworkInspect(1)));
	}

	#[test]
	fn test_toggle_network_mark() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://c.com", "GET", ewd_core::events::NetworkEventType::Finished),
		];

		state.toggle_network_mark();
		assert!(state.marked_network_indices.contains(&0));
		assert_eq!(state.marked_network_indices.len(), 1);

		state.network_move_down();
		state.toggle_network_mark();
		assert!(state.marked_network_indices.contains(&0));
		assert!(state.marked_network_indices.contains(&1));
		assert_eq!(state.marked_network_indices.len(), 2);

		state.toggle_network_mark();
		assert!(state.marked_network_indices.contains(&0));
		assert!(!state.marked_network_indices.contains(&1));
		assert_eq!(state.marked_network_indices.len(), 1);
	}

	#[test]
	fn test_marked_events() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://c.com", "GET", ewd_core::events::NetworkEventType::Finished),
		];

		state.marked_network_indices.insert(0);
		state.marked_network_indices.insert(2);

		let marked = state.marked_events();
		assert_eq!(marked.len(), 2);
		assert_eq!(marked[0].url, "https://a.com");
		assert_eq!(marked[1].url, "https://c.com");
	}

	#[test]
	fn test_clear_network_marks() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Finished),
		];

		state.marked_network_indices.insert(0);
		state.marked_network_indices.insert(1);
		assert_eq!(state.marked_network_indices.len(), 2);

		state.clear_network_marks();
		assert_eq!(state.marked_network_indices.len(), 0);
	}

	#[test]
	fn test_marked_indices_shift_on_trim() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://c.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://d.com", "POST", ewd_core::events::NetworkEventType::Finished),
		];

		state.marked_network_indices.insert(1);
		state.marked_network_indices.insert(3);

		state.trim_network_history(2);

		assert_eq!(state.marked_network_indices.len(), 1);
		assert!(state.marked_network_indices.contains(&1));
		assert_eq!(state.network_events.len(), 2);
		assert_eq!(state.marked_events()[0].url, "https://d.com");
	}

	#[test]
	fn test_trim_drops_removed_marks() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://c.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://d.com", "POST", ewd_core::events::NetworkEventType::Finished),
		];

		state.marked_network_indices.insert(2);
		state.marked_network_indices.insert(3);

		state.trim_network_history(2);

		assert!(state.marked_network_indices.contains(&0));
		assert!(state.marked_network_indices.contains(&1));
		assert_eq!(state.marked_network_indices.len(), 2);

		let marked = state.marked_events();
		assert_eq!(marked[0].url, "https://c.com");
		assert_eq!(marked[1].url, "https://d.com");
	}

	#[test]
	fn test_trim_clamps_cursor() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://c.com", "GET", ewd_core::events::NetworkEventType::Finished),
		];
		state.network_cursor = 2;

		state.trim_network_history(2);
		assert_eq!(state.network_cursor, 0);
	}

	#[test]
	fn test_trim_zero_is_noop() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Finished),
		];
		state.marked_network_indices.insert(1);

		state.trim_network_history(0);

		assert_eq!(state.network_events.len(), 2);
		assert!(state.marked_network_indices.contains(&1));
	}

	#[test]
	fn test_started_collapsed_when_finished_exists() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Started),
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Finished),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Started),
		];

		let visible = state.visible_network_indices();
		assert_eq!(visible, vec![1, 2]);
	}

	#[test]
	fn test_started_visible_when_no_finished() {
		let mut state = mock_state();
		state.network_events = vec![
			make_network_event("https://a.com", "GET", ewd_core::events::NetworkEventType::Started),
			make_network_event("https://b.com", "POST", ewd_core::events::NetworkEventType::Started),
		];

		let visible = state.visible_network_indices();
		assert_eq!(visible, vec![0, 1]);
	}
}
