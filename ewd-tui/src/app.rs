use anyhow::{Context, Result};
use ratatui::style::{Color, Style};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;

use crate::build::{build_esbuild_command, run_esbuild, BuildOutput};
use crate::ipc::IpcClient;
use crate::keys::Keymap;
use crate::projects::{create_project, delete_project, get_bundle_mtime, get_sources_mtime, list_projects, Project};
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateProjectField {
	Name,
	Dir,
	Url,
	Entry,
}

#[derive(Debug, Clone)]
pub struct InputField {
	pub text: String,
	pub cursor: usize,
	pub anchor: usize,
}

impl InputField {
	pub fn new() -> Self {
		Self {
			text: String::new(),
			cursor: 0,
			anchor: 0,
		}
	}

	pub fn has_selection(&self) -> bool {
		self.cursor != self.anchor
	}

	pub fn selection_range(&self) -> Option<(usize, usize)> {
		if self.has_selection() {
			Some((self.cursor.min(self.anchor), self.cursor.max(self.anchor)))
		} else {
			None
		}
	}

	fn delete_selection(&mut self) -> String {
		if let Some((start, end)) = self.selection_range() {
			let deleted: String = self.text.chars().skip(start).take(end - start).collect();
			let before: String = self.text.chars().take(start).collect();
			let after: String = self.text.chars().skip(end).collect();
			self.text = before + &after;
			self.cursor = start;
			self.anchor = start;
			deleted
		} else {
			String::new()
		}
	}

	pub fn insert(&mut self, c: char) {
		self.delete_selection();
		let byte_idx = self
			.text
			.char_indices()
			.nth(self.cursor)
			.map(|(i, _)| i)
			.unwrap_or(self.text.len());
		self.text.insert(byte_idx, c);
		self.cursor += 1;
		self.anchor = self.cursor;
	}

	pub fn backspace(&mut self) {
		if self.has_selection() {
			self.delete_selection();
		} else if self.cursor > 0 {
			self.cursor -= 1;
			let before: String = self.text.chars().take(self.cursor).collect();
			let after: String = self.text.chars().skip(self.cursor + 1).collect();
			self.text = before + &after;
			self.anchor = self.cursor;
		}
	}

	pub fn delete(&mut self) {
		if self.has_selection() {
			self.delete_selection();
		} else if self.cursor < self.text.chars().count() {
			let before: String = self.text.chars().take(self.cursor).collect();
			let after: String = self.text.chars().skip(self.cursor + 1).collect();
			self.text = before + &after;
		}
	}

	pub fn move_left(&mut self) {
		if self.cursor > 0 {
			self.cursor -= 1;
			self.anchor = self.cursor;
		}
	}

	pub fn move_right(&mut self) {
		if self.cursor < self.text.chars().count() {
			self.cursor += 1;
			self.anchor = self.cursor;
		}
	}

	pub fn move_to_line_start(&mut self) {
		self.cursor = 0;
		self.anchor = 0;
	}

	pub fn move_to_line_end(&mut self) {
		self.cursor = self.text.chars().count();
		self.anchor = self.cursor;
	}

	pub fn move_word_left(&mut self) {
		if self.cursor == 0 {
			return;
		}
		let chars: Vec<char> = self.text.chars().collect();
		let mut pos = self.cursor;
		while pos > 0 && chars[pos - 1].is_whitespace() {
			pos -= 1;
		}
		while pos > 0 && !chars[pos - 1].is_whitespace() {
			pos -= 1;
		}
		self.cursor = pos;
		self.anchor = pos;
	}

	pub fn move_word_right(&mut self) {
		let chars: Vec<char> = self.text.chars().collect();
		if self.cursor >= chars.len() {
			return;
		}
		let mut pos = self.cursor;
		while pos < chars.len() && !chars[pos].is_whitespace() {
			pos += 1;
		}
		while pos < chars.len() && chars[pos].is_whitespace() {
			pos += 1;
		}
		self.cursor = pos;
		self.anchor = pos;
	}

	pub fn select_left(&mut self) {
		if self.cursor > 0 {
			self.cursor -= 1;
		}
	}

	pub fn select_right(&mut self) {
		if self.cursor < self.text.chars().count() {
			self.cursor += 1;
		}
	}

	pub fn select_to_line_start(&mut self) {
		self.cursor = 0;
	}

	pub fn select_to_line_end(&mut self) {
		self.cursor = self.text.chars().count();
	}

	pub fn select_word_left(&mut self) {
		if self.cursor == 0 {
			return;
		}
		let chars: Vec<char> = self.text.chars().collect();
		let mut pos = self.cursor;
		while pos > 0 && chars[pos - 1].is_whitespace() {
			pos -= 1;
		}
		while pos > 0 && !chars[pos - 1].is_whitespace() {
			pos -= 1;
		}
		self.cursor = pos;
	}

	pub fn select_word_right(&mut self) {
		let chars: Vec<char> = self.text.chars().collect();
		if self.cursor >= chars.len() {
			return;
		}
		let mut pos = self.cursor;
		while pos < chars.len() && !chars[pos].is_whitespace() {
			pos += 1;
		}
		while pos < chars.len() && chars[pos].is_whitespace() {
			pos += 1;
		}
		self.cursor = pos;
	}

	pub fn select_all(&mut self) {
		self.anchor = 0;
		self.cursor = self.text.chars().count();
	}

	pub fn paste(&mut self, text: &str, is_url: bool) {
		self.delete_selection();
		let sanitized: String = if is_url {
			text.chars().filter(|c| !c.is_whitespace()).collect()
		} else {
			text.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).collect()
		};
		let byte_idx = self
			.text
			.char_indices()
			.nth(self.cursor)
			.map(|(i, _)| i)
			.unwrap_or(self.text.len());
		self.text.insert_str(byte_idx, &sanitized);
		self.cursor += sanitized.chars().count();
		self.anchor = self.cursor;
	}

	pub fn copy(&self) -> String {
		if let Some((start, end)) = self.selection_range() {
			self.text.chars().skip(start).take(end - start).collect()
		} else {
			self.text.clone()
		}
	}

	pub fn cut(&mut self) -> String {
		if self.has_selection() {
			self.delete_selection()
		} else {
			let all = self.text.clone();
			self.text.clear();
			self.cursor = 0;
			self.anchor = 0;
			all
		}
	}

	pub fn cut_word(&mut self) -> String {
		if self.has_selection() {
			return self.delete_selection();
		}
		let chars: Vec<char> = self.text.chars().collect();
		if self.cursor == 0 {
			return String::new();
		}
		let mut start = self.cursor;
		while start > 0 && chars[start - 1].is_whitespace() {
			start -= 1;
		}
		while start > 0 && !chars[start - 1].is_whitespace() {
			start -= 1;
		}
		let end = self.cursor;
		if start == end {
			return String::new();
		}
		let word: String = chars[start..end].iter().collect();
		let before: String = chars[..start].iter().collect();
		let after: String = chars[end..].iter().collect();
		self.text = before + &after;
		self.cursor = start;
		self.anchor = start;
		word
	}

	pub fn kill_line(&mut self) -> String {
		if self.has_selection() {
			return self.delete_selection();
		}
		let chars: Vec<char> = self.text.chars().collect();
		if self.cursor >= chars.len() {
			return String::new();
		}
		let killed: String = chars[self.cursor..].iter().collect();
		self.text = chars[..self.cursor].iter().collect();
		self.anchor = self.cursor;
		killed
	}

	pub fn backward_kill_line(&mut self) -> String {
		if self.has_selection() {
			return self.delete_selection();
		}
		if self.cursor == 0 {
			return String::new();
		}
		let chars: Vec<char> = self.text.chars().collect();
		let killed: String = chars[..self.cursor].iter().collect();
		self.text = chars[self.cursor..].iter().collect();
		self.cursor = 0;
		self.anchor = 0;
		killed
	}
}

#[derive(Debug, Clone)]
pub struct CreateProjectState {
	pub name: InputField,
	pub dir: InputField,
	pub url: InputField,
	pub entry: InputField,
	pub focused_field: CreateProjectField,
	pub invalid_fields: Vec<CreateProjectField>,
	pub error: Option<String>,
}

impl Default for CreateProjectState {
	fn default() -> Self {
		Self {
			name: InputField::new(),
			dir: InputField::new(),
			url: InputField::new(),
			entry: InputField::new(),
			focused_field: CreateProjectField::Name,
			invalid_fields: Vec::new(),
			error: None,
		}
	}
}

/// Результат фоновой сборки, передаётся через канал
#[derive(Debug)]
pub struct BuildResult {
	pub dir: String,
	pub output: Result<BuildOutput>,
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
	pub build_rx: Option<mpsc::UnboundedReceiver<BuildResult>>,
	build_tx: mpsc::UnboundedSender<BuildResult>,
}

impl AppState {
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

	pub async fn new(log_rx: mpsc::UnboundedReceiver<String>) -> Result<Self> {
		let mut projects = Vec::new();
		for (project, mtime) in list_projects()? {
			projects.push(ProjectState {
				project,
				status: ProjectStatus::Stopped,
				tab_id: None,
				mtime,
				tab_visible: false,
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

	pub fn filtered_indices(&self) -> Vec<usize> {
		if !self.search_mode || self.search_query.is_empty() {
			return (0..self.projects.len()).collect();
		}
		let query = self.search_query.to_lowercase();
		self.projects
			.iter()
			.enumerate()
			.filter(|(_, p)| p.project.config.name.to_lowercase().contains(&query))
			.map(|(i, _)| i)
			.collect()
	}

	pub fn filtered_projects(&self) -> Vec<&ProjectState> {
		self.filtered_indices()
			.into_iter()
			.filter_map(|i| self.projects.get(i))
			.collect()
	}

	fn selected_real_index(&self) -> Option<usize> {
		self.filtered_indices().get(self.selected_index).copied()
	}

	pub fn navigate_up(&mut self) {
		if self.selected_index > 0 {
			self.selected_index -= 1;
		}
	}

	pub fn navigate_down(&mut self) {
		let count = self.filtered_indices().len();
		if self.selected_index < count.saturating_sub(1) {
			self.selected_index += 1;
		}
	}

	pub fn enter_project(&mut self) {
		if let Some(real) = self.selected_real_index() {
			if let Some(project) = self.projects.get(real) {
				let dir = project.project.dir.clone();
				self.selected_index = real;
				self.search_mode = false;
				self.search_query.clear();
				self.console_buffer.clear();
				self.screen = Screen::Project(dir.clone());

				if let Err(e) = self.ensure_project_running(&dir) {
					log::error!("Failed to ensure project running: {}", e);
				}
			}
		}
	}

	/// Проверяет, нужна ли сборка, и планирует запуск проекта
	pub fn ensure_project_running(&mut self, dir: &str) -> Result<()> {
		if let Some(project) = self.projects.iter().find(|p| p.project.dir == dir) {
			if project.status == ProjectStatus::Running {
				return Ok(());
			}
		}

		self.pending_start = Some(dir.to_string());

		let bundle_mtime = get_bundle_mtime(dir);
		let sources_mtime = get_sources_mtime(dir);

		let needs_build = match (bundle_mtime, sources_mtime) {
			(Err(_), _) => {
				log::info!("Bundle not found for '{}', building", dir);
				true
			}
			(Ok(bundle), Ok(sources)) => {
				if sources > bundle {
					log::info!("Sources newer than bundle for '{}', building", dir);
					true
				} else {
					log::info!("Bundle up-to-date for '{}', skipping build", dir);
					false
				}
			}
			(Ok(_), Err(e)) => {
				log::warn!("Failed to get sources mtime for '{}': {}", dir, e);
				true
			}
		};

		if needs_build {
			self.start_build(dir)?;
		}

		Ok(())
	}

	/// Запускает фоновую сборку через tokio::spawn
	pub fn start_build(&mut self, dir: &str) -> Result<()> {
		let project = match self.projects.iter().find(|p| p.project.dir == dir) {
			Some(p) => p.project.clone(),
			None => return Ok(()),
		};

		self.console_buffer.clear();
		self.is_building = true;

		let (cmd, args) = build_esbuild_command(&project)?;
		self.console_buffer.push_line_styled(
			format!("$ {} {}", cmd, args.join(" ")),
			Style::default().fg(Color::DarkGray),
		);

		let tx = self.build_tx.clone();
		let dir_owned = dir.to_string();
		tokio::spawn(async move {
			let output = run_esbuild(&project).await;
			let _ = tx.send(BuildResult { dir: dir_owned, output });
		});

		Ok(())
	}

	/// Забирает результаты сборки из канала и обновляет Console
	pub fn drain_build_results(&mut self) {
		let rx = match self.build_rx.as_mut() {
			Some(rx) => rx,
			None => return,
		};

		loop {
			match rx.try_recv() {
				Ok(result) => {
					match result.output {
						Ok(output) => {
							for line in output.stdout.lines() {
								self.console_buffer.push_ansi_line(line.to_string());
							}
							for line in output.stderr.lines() {
								self.console_buffer.push_ansi_line(line.to_string());
							}

							if output.success {
								let msg = format!("Сборка успешна: {}", output.bundle_path.display());
								self.console_buffer
									.push_line_styled(msg, Style::default().fg(Color::Green));

								if let Some(project) = self.projects.iter_mut().find(|p| p.project.dir == result.dir) {
									project.mtime = SystemTime::now();
								}

								// Если проект ожидал запуска после сборки — запускаем
								if self.pending_start.as_deref() == Some(&result.dir) {
									self.pending_start = None;
								} else {
									// Иначе перезапускаем таб, если он уже запущен
									let should_restart = self
										.projects
										.iter()
										.find(|p| p.project.dir == result.dir)
										.map(|p| p.status == ProjectStatus::Running)
										.unwrap_or(false);
									if should_restart {
										let dir = result.dir.clone();
										// Перезапуск — асинхронный, планируем через spawn
										// (в event loop будет обработано через pending_start)
										self.pending_start = Some(dir);
									}
								}
							} else {
								self.console_buffer
									.push_line_styled("Сборка не удалась".to_string(), Style::default().fg(Color::Red));
								if self.pending_start.as_deref() == Some(&result.dir) {
									self.pending_start = None;
								}
							}
						}
						Err(e) => {
							self.console_buffer
								.push_line_styled(format!("Ошибка сборки: {}", e), Style::default().fg(Color::Red));
							if self.pending_start.as_deref() == Some(&result.dir) {
								self.pending_start = None;
							}
						}
					}
					self.is_building = false;
				}
				Err(mpsc::error::TryRecvError::Empty) => break,
				Err(mpsc::error::TryRecvError::Disconnected) => break,
			}
		}
	}

	pub async fn start_project(&mut self, dir: &str) -> Result<()> {
		if let Some(ipc) = &self.ipc_client {
			if let Some(project) = self.projects.iter_mut().find(|p| p.project.dir == dir) {
				if project.status == ProjectStatus::Stopped {
					let tab_id = ipc.create(&project.project.config.url, project.tab_visible).await?;
					project.tab_id = Some(tab_id);
					project.status = ProjectStatus::Running;
					self.start_watcher(dir);
				}
			}
		}
		Ok(())
	}

	pub async fn stop_project(&mut self, dir: &str) -> Result<()> {
		if let Some(ipc) = &self.ipc_client {
			if let Some(project) = self.projects.iter_mut().find(|p| p.project.dir == dir) {
				if project.status == ProjectStatus::Running {
					if let Some(ref tab_id) = project.tab_id {
						let _ = ipc.close(tab_id).await;
					}
					project.tab_id = None;
					project.status = ProjectStatus::Stopped;
					self.stop_watcher(dir);
				}
			}
		}
		Ok(())
	}

	pub async fn toggle_tab_visibility(&mut self, dir: &str) -> Result<()> {
		if let Some(ipc) = &self.ipc_client {
			if let Some(project) = self.projects.iter_mut().find(|p| p.project.dir == dir) {
				if project.status == ProjectStatus::Running {
					if project.tab_visible {
						if let Some(ref tab_id) = project.tab_id {
							ipc.hide(tab_id).await?;
						}
						project.tab_visible = false;
					} else {
						if let Some(ref tab_id) = project.tab_id {
							ipc.show(tab_id).await?;
						}
						project.tab_visible = true;
					}
				}
			}
		}
		Ok(())
	}

	pub async fn stop_and_exit_project(&mut self) -> Result<()> {
		if let Screen::Project(dir) = &self.screen {
			let dir = dir.clone();
			self.pending_start = None;
			self.stop_project(&dir).await?;
		}
		self.screen = Screen::Projects;
		Ok(())
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

	pub fn start_watcher(&mut self, dir: &str) {
		if self.active_watchers.contains_key(dir) {
			return;
		}
		if let Some(project) = self.projects.iter().find(|p| p.project.dir == dir) {
			let watcher = ProjectWatcher::start(project.project.clone(), self.watcher_tx.clone());
			self.active_watchers.insert(dir.to_string(), watcher);
			log::info!("Watcher started for '{}'", dir);
		}
	}

	pub fn stop_watcher(&mut self, dir: &str) {
		if let Some(watcher) = self.active_watchers.remove(dir) {
			watcher.stop();
			log::info!("Watcher stopped for '{}'", dir);
		}
	}

	pub async fn drain_watcher_events(&mut self) {
		loop {
			match self.watcher_rx.as_mut().map(|rx| rx.try_recv()) {
				Some(Ok(event)) => {
					log::info!("Build event for '{}'", event.dir);
					if let Err(e) = self.start_build(&event.dir) {
						log::error!("Failed to start build: {}", e);
					}
				}
				Some(Err(_)) => break,
				None => break,
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

	pub fn get_active_buffer_mut(&mut self) -> &mut TextBuffer {
		match self.active_buffer {
			BufferKind::Console => &mut self.console_buffer,
			BufferKind::Logs => &mut self.log_buffer,
		}
	}

	pub fn buffer_move_up(&mut self) {
		self.get_active_buffer_mut().move_up();
	}
	pub fn buffer_move_down(&mut self) {
		self.get_active_buffer_mut().move_down();
	}
	pub fn buffer_move_left(&mut self) {
		self.get_active_buffer_mut().move_left();
	}
	pub fn buffer_move_right(&mut self) {
		self.get_active_buffer_mut().move_right();
	}
	pub fn buffer_page_up(&mut self, page_size: usize) {
		self.get_active_buffer_mut().page_up(page_size);
	}
	pub fn buffer_page_down(&mut self, page_size: usize) {
		self.get_active_buffer_mut().page_down(page_size);
	}
	pub fn buffer_home(&mut self) {
		self.get_active_buffer_mut().move_to_buffer_start();
	}
	pub fn buffer_end(&mut self) {
		self.get_active_buffer_mut().move_to_buffer_end();
	}
	pub fn buffer_select_up(&mut self) {
		self.get_active_buffer_mut().select_up();
	}
	pub fn buffer_select_down(&mut self) {
		self.get_active_buffer_mut().select_down();
	}
	pub fn buffer_select_left(&mut self) {
		self.get_active_buffer_mut().select_left();
	}
	pub fn buffer_select_right(&mut self) {
		self.get_active_buffer_mut().select_right();
	}
	pub fn buffer_select_all(&mut self) {
		self.get_active_buffer_mut().select_all();
	}
	pub fn buffer_toggle_mark(&mut self) {
		self.get_active_buffer_mut().toggle_mark();
	}
	pub fn buffer_copy(&mut self) {
		if let Some(text) = self.get_active_buffer_mut().selected_text() {
			if let Some(clipboard) = &mut self.clipboard {
				let _ = clipboard.set_text(text);
			}
		}
	}

	pub fn request_delete(&mut self) {
		if let Some(real) = self.selected_real_index() {
			if let Some(project) = self.projects.get(real) {
				self.confirm_delete = Some(project.project.dir.clone());
				self.screen = Screen::ConfirmDelete;
			}
		}
	}

	pub fn confirm_delete(&mut self) -> Result<()> {
		if let Some(dir) = self.confirm_delete.take() {
			self.stop_watcher(&dir);
			delete_project(&dir)?;
			self.projects.retain(|p| p.project.dir != dir);
			let visible = self.filtered_indices().len();
			if visible == 0 {
				self.selected_index = 0;
			} else if self.selected_index >= visible {
				self.selected_index = visible - 1;
			}
		}
		self.screen = Screen::Projects;
		Ok(())
	}

	pub fn cancel_delete(&mut self) {
		self.confirm_delete = None;
		self.screen = Screen::Projects;
	}

	pub fn enter_search(&mut self) {
		self.search_mode = true;
		self.search_query.clear();
		self.selected_index = 0;
	}

	pub fn exit_search(&mut self) {
		let real = self.selected_real_index().unwrap_or(0);
		self.search_mode = false;
		self.search_query.clear();
		self.selected_index = real.min(self.projects.len().saturating_sub(1));
	}

	pub fn search_input_char(&mut self, c: char) {
		self.search_query.push(c);
		self.selected_index = 0;
	}

	pub fn search_backspace(&mut self) {
		self.search_query.pop();
		self.selected_index = 0;
	}

	pub fn search_submit(&mut self) {
		self.enter_project();
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
