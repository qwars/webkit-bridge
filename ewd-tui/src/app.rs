use anyhow::{Context, Result};

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;

use crate::build::BuildOutput;
use crate::create_project::{CreateProjectField, CreateProjectState};
use crate::input_field::InputField;
use crate::ipc::IpcClient;
use crate::keys::Keymap;
use crate::projects::{create_project, list_projects, Project};
use crate::text_buffer::TextBuffer;
use crate::watcher::{BuildEvent, ProjectWatcher};

const MAX_LOG_LINES: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
	Projects,
	Project(String),
	CreateProject,
	ConfirmDelete,
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

/// Результат фоновой сборки, передаётся через канал
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
}

impl AppState {
	pub async fn new(log_rx: mpsc::UnboundedReceiver<String>) -> Result<Self> {
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
			loop {
				match rx.try_recv() {
					Ok(line) => {
						self.log_buffer.push_line(line);
					}
					Err(_) => break,
				}
			}
		}
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

	fn mock_state() -> AppState {
		let (watcher_tx, watcher_rx) = mpsc::unbounded_channel();
		let (build_tx, build_rx) = mpsc::unbounded_channel();
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
		}
	}

	#[test]
	fn test_input_field_selection() {
		let mut field = InputField::new();
		for c in "hello".chars() {
			field.insert(c);
		}
		assert!(!field.has_selection());
		field.select_all();
		assert_eq!(field.selection_range(), Some((0, 5)));
		field.paste("world", false);
		assert_eq!(field.text, "world");
		assert!(!field.has_selection());
	}

	#[test]
	fn test_input_field_cut_word() {
		let mut field = InputField::new();
		for c in "hello world".chars() {
			field.insert(c);
		}
		let word = field.cut_word();
		assert_eq!(word, "world");
		assert_eq!(field.text, "hello ");
	}

	#[test]
	fn test_input_field_kill_line() {
		let mut field = InputField::new();
		for c in "hello world".chars() {
			field.insert(c);
		}
		field.move_to_line_start();
		field.move_word_right();
		let killed = field.kill_line();
		assert_eq!(killed, "world");
		assert_eq!(field.text, "hello ");
	}

	#[test]
	fn test_navigate_up() {
		let mut state = mock_state();
		state.selected_index = 1;
		state.navigate_up();
		assert_eq!(state.selected_index, 0);
	}

	#[test]
	fn test_navigate_down() {
		let mut state = mock_state();
		state.navigate_down();
		assert_eq!(state.selected_index, 1);
	}

	#[tokio::test]
	async fn test_enter_project() {
		let mut state = mock_state();
		state.enter_project();
		assert_eq!(state.screen, Screen::Project("proj-a".to_string()));
	}

	#[test]
	fn test_current_project() {
		let mut state = mock_state();
		state.screen = Screen::Project("proj-a".to_string());
		assert_eq!(state.current_project().unwrap().project.config.name, "Project A");
		state.screen = Screen::Projects;
		assert!(state.current_project().is_none());
	}

	#[test]
	fn test_drain_logs_with_data() {
		let (tx, rx) = mpsc::unbounded_channel();
		tx.send("line 1".to_string()).unwrap();
		tx.send("line 2".to_string()).unwrap();
		let mut state = mock_state();
		state.log_rx = Some(rx);
		state.drain_logs();
		assert_eq!(
			state
				.log_buffer
				.lines
				.iter()
				.map(|l| l.text.as_str())
				.collect::<Vec<_>>(),
			vec!["line 1", "line 2"]
		);
	}

	#[test]
	fn test_create_project_input_and_cursor() {
		let mut state = mock_state();
		state.start_create_project();
		assert_eq!(state.screen, Screen::CreateProject);
		for c in "test".chars() {
			state.input_char(c);
		}
		assert_eq!(state.create_project_state.name.text, "test");
		assert_eq!(state.create_project_state.name.cursor, 4);
		state.move_cursor_left();
		state.move_cursor_left();
		assert_eq!(state.create_project_state.name.cursor, 2);
		state.backspace();
		assert_eq!(state.create_project_state.name.text, "tst");
		state.next_field();
		assert_eq!(state.create_project_state.focused_field, CreateProjectField::Dir);
	}

	#[test]
	fn test_search_filters_projects() {
		let mut state = mock_state();
		state.enter_search();
		for c in "b".chars() {
			state.search_input_char(c);
		}
		let visible = state.filtered_projects();
		assert_eq!(visible.len(), 1);
		assert_eq!(visible[0].project.config.name, "Project B");
	}

	#[test]
	fn test_search_case_insensitive() {
		let mut state = mock_state();
		state.enter_search();
		for c in "PROJECT a".chars() {
			state.search_input_char(c);
		}
		let visible = state.filtered_projects();
		assert_eq!(visible.len(), 1);
		assert_eq!(visible[0].project.dir, "proj-a");
	}

	#[test]
	fn test_search_navigation_uses_filtered_list() {
		let mut state = mock_state();
		state.enter_search();
		assert_eq!(state.filtered_indices().len(), 2);
		state.navigate_down();
		assert_eq!(state.selected_index, 1);
		state.navigate_down();
		assert_eq!(state.selected_index, 1);
	}

	#[tokio::test]
	async fn test_search_submit_enters_project() {
		let mut state = mock_state();
		state.enter_search();
		state.search_input_char('b');
		state.search_submit();
		assert_eq!(state.screen, Screen::Project("proj-b".to_string()));
		assert!(!state.search_mode);
		assert!(state.search_query.is_empty());
	}

	#[test]
	fn test_search_exit_restores_selection() {
		let mut state = mock_state();
		state.enter_search();
		state.search_input_char('b');
		state.exit_search();
		assert!(!state.search_mode);
		assert_eq!(state.selected_index, 1);
	}

	#[test]
	fn test_search_backspace_widens_filter() {
		let mut state = mock_state();
		state.enter_search();
		state.search_input_char('b');
		assert_eq!(state.filtered_projects().len(), 1);
		state.search_backspace();
		assert_eq!(state.filtered_projects().len(), 2);
	}
}
