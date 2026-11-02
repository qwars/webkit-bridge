use anyhow::{bail, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Keymap {
	Emacs,
	Vim,
	Vscode,
}

impl Default for Keymap {
	fn default() -> Self {
		Keymap::Emacs
	}
}

impl std::str::FromStr for Keymap {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self> {
		match s.to_lowercase().as_str() {
			"emacs" => Ok(Keymap::Emacs),
			"vim" => Ok(Keymap::Vim),
			"vscode" => Ok(Keymap::Vscode),
			_ => bail!("Invalid keymap: {}. Use 'emacs', 'vim', or 'vscode'", s),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyContext {
	Global,
	Projects,
	ProjectSearch,
	ProjectTabs,
	CreateProject,
	BufferView,
	ConfirmDelete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
	Quit,
	ShowMenu,
	ToggleKeymap,
	NavigateUp,
	NavigateDown,
	EnterProject,
	RestartProject,
	NewProject,
	DeleteProject,
	SwitchToConsole,
	SwitchToNetwork,
	SwitchToResources,
	SwitchToDom,
	SwitchToLogs,
	ExitProject,
	ToggleTabVisibility,

	InputChar(char),
	Backspace,
	Delete,
	NextField,
	SubmitForm,
	CancelForm,

	MoveLeft,
	MoveRight,
	MoveToLineStart,
	MoveToLineEnd,
	MoveWordLeft,
	MoveWordRight,

	SelectLeft,
	SelectRight,
	SelectToLineStart,
	SelectToLineEnd,
	SelectWordLeft,
	SelectWordRight,
	SelectAll,

	Copy,
	Cut,
	Paste,
	CutWord,
	KillLine,
	BackwardKillLine,

	BufferMoveUp,
	BufferMoveDown,
	BufferMoveLeft,
	BufferMoveRight,
	BufferPageUp,
	BufferPageDown,
	BufferHome,
	BufferEnd,
	BufferSelectUp,
	BufferSelectDown,
	BufferSelectLeft,
	BufferSelectRight,
	BufferSelectAll,
	BufferToggleMark,
	BufferCopy,

	EnterSearch,
	ExitSearch,
	SearchChar(char),
	SearchBackspace,
	SearchSubmit,

	ConfirmDelete,
	CancelDelete,
}

pub fn key_to_action(key: KeyEvent, context: KeyContext, keymap: Keymap) -> Option<Action> {
	match context {
		KeyContext::Global => global_action(key),
		KeyContext::Projects => projects_action(key),
		KeyContext::ProjectSearch => project_search_action(key),
		KeyContext::ProjectTabs => project_tabs_action(key),
		KeyContext::CreateProject => create_project_action(key, keymap),
		KeyContext::BufferView => buffer_view_action(key, keymap),
		KeyContext::ConfirmDelete => confirm_delete_action(key),
	}
}

fn global_action(key: KeyEvent) -> Option<Action> {
	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	match (key.code, ctrl) {
		(KeyCode::Char('q'), false) => Some(Action::Quit),
		(KeyCode::Char('?'), false) => Some(Action::ShowMenu),
		(KeyCode::Char('k'), true) => Some(Action::ToggleKeymap),
		_ => None,
	}
}

fn projects_action(key: KeyEvent) -> Option<Action> {
	match key.code {
		KeyCode::Up => Some(Action::NavigateUp),
		KeyCode::Down => Some(Action::NavigateDown),
		KeyCode::Enter => Some(Action::EnterProject),
		KeyCode::Char('/') => Some(Action::EnterSearch),
		KeyCode::Char('n') => Some(Action::NewProject),
		KeyCode::Char('d') => Some(Action::DeleteProject),
		_ => None,
	}
}

fn project_search_action(key: KeyEvent) -> Option<Action> {
	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	let alt = key.modifiers.contains(KeyModifiers::ALT);

	match key.code {
		KeyCode::Esc => return Some(Action::ExitSearch),
		KeyCode::Enter => return Some(Action::SearchSubmit),
		KeyCode::Backspace => return Some(Action::SearchBackspace),
		KeyCode::Up => return Some(Action::NavigateUp),
		KeyCode::Down => return Some(Action::NavigateDown),
		_ => {}
	}

	if !ctrl && !alt {
		if let KeyCode::Char(c) = key.code {
			return Some(Action::SearchChar(c));
		}
	}

	None
}

/// Переключение вкладок и действий уровня проекта — общее для ProjectTabs и BufferView
fn tab_switch_action(key: KeyEvent) -> Option<Action> {
	if !key.modifiers.contains(KeyModifiers::SHIFT) {
		return None;
	}
	match key.code {
		KeyCode::Char('c') | KeyCode::Char('C') => Some(Action::SwitchToConsole),
		KeyCode::Char('n') | KeyCode::Char('N') => Some(Action::SwitchToNetwork),
		KeyCode::Char('r') | KeyCode::Char('R') => Some(Action::SwitchToResources),
		KeyCode::Char('d') | KeyCode::Char('D') => Some(Action::SwitchToDom),
		KeyCode::Char('l') | KeyCode::Char('L') => Some(Action::SwitchToLogs),
		KeyCode::Char('v') | KeyCode::Char('V') => Some(Action::ToggleTabVisibility),
		_ => None,
	}
}

fn project_tabs_action(key: KeyEvent) -> Option<Action> {
	if let Some(action) = tab_switch_action(key) {
		return Some(action);
	}

	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	match (key.code, ctrl) {
		(KeyCode::F(5), false) => Some(Action::RestartProject),
		(KeyCode::Char('r'), true) => Some(Action::RestartProject),
		(KeyCode::Esc, _) => Some(Action::ExitProject),
		_ => None,
	}
}

fn confirm_delete_action(key: KeyEvent) -> Option<Action> {
	match key.code {
		KeyCode::Char('y') | KeyCode::Char('Y') => Some(Action::ConfirmDelete),
		KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Some(Action::CancelDelete),
		_ => None,
	}
}

fn create_project_action(key: KeyEvent, keymap: Keymap) -> Option<Action> {
	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	let alt = key.modifiers.contains(KeyModifiers::ALT);
	let shift = key.modifiers.contains(KeyModifiers::SHIFT);

	if shift && !ctrl && !alt {
		match key.code {
			KeyCode::Left => return Some(Action::SelectLeft),
			KeyCode::Right => return Some(Action::SelectRight),
			KeyCode::Home => return Some(Action::SelectToLineStart),
			KeyCode::End => return Some(Action::SelectToLineEnd),
			_ => {}
		}
	}

	if !ctrl && !alt {
		if let KeyCode::Char(c) = key.code {
			return Some(Action::InputChar(c));
		}
	}

	match keymap {
		Keymap::Emacs => emacs_create_project_action(key),
		Keymap::Vim => vim_create_project_action(key),
		Keymap::Vscode => vscode_create_project_action(key),
	}
}

fn emacs_create_project_action(key: KeyEvent) -> Option<Action> {
	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	let alt = key.modifiers.contains(KeyModifiers::ALT);
	let shift = key.modifiers.contains(KeyModifiers::SHIFT);

	match (key.code, ctrl, alt, shift) {
		(KeyCode::Char('a'), true, false, false) => Some(Action::MoveToLineStart),
		(KeyCode::Char('e'), true, false, false) => Some(Action::MoveToLineEnd),
		(KeyCode::Char('b'), true, false, false) => Some(Action::MoveLeft),
		(KeyCode::Char('f'), true, false, false) => Some(Action::MoveRight),
		(KeyCode::Char('w'), true, false, false) => Some(Action::CutWord),
		(KeyCode::Char('y'), true, false, false) => Some(Action::Paste),
		(KeyCode::Char('k'), true, false, false) => Some(Action::KillLine),
		(KeyCode::Char('u'), true, false, false) => Some(Action::BackwardKillLine),

		(KeyCode::Char('b'), false, true, false) => Some(Action::MoveWordLeft),
		(KeyCode::Char('f'), false, true, false) => Some(Action::MoveWordRight),
		(KeyCode::Char('w'), false, true, false) => Some(Action::Copy),

		(KeyCode::Char('b') | KeyCode::Char('B'), false, true, true) => Some(Action::SelectWordLeft),
		(KeyCode::Char('f') | KeyCode::Char('F'), false, true, true) => Some(Action::SelectWordRight),

		(KeyCode::Left, false, false, false) => Some(Action::MoveLeft),
		(KeyCode::Right, false, false, false) => Some(Action::MoveRight),
		(KeyCode::Home, false, false, false) => Some(Action::MoveToLineStart),
		(KeyCode::End, false, false, false) => Some(Action::MoveToLineEnd),
		(KeyCode::Backspace, false, false, false) => Some(Action::Backspace),
		(KeyCode::Delete, false, false, false) => Some(Action::Delete),
		(KeyCode::Esc, false, false, false) => Some(Action::CancelForm),
		(KeyCode::Enter, false, false, false) => Some(Action::SubmitForm),
		(KeyCode::Tab, false, false, false) => Some(Action::NextField),

		_ => None,
	}
}

fn vim_create_project_action(key: KeyEvent) -> Option<Action> {
	let alt = key.modifiers.contains(KeyModifiers::ALT);
	let shift = key.modifiers.contains(KeyModifiers::SHIFT);

	match (key.code, alt, shift) {
		(KeyCode::Char('b'), true, false) => Some(Action::MoveWordLeft),
		(KeyCode::Char('w'), true, false) => Some(Action::MoveWordRight),
		(KeyCode::Char('0'), true, false) => Some(Action::MoveToLineStart),
		(KeyCode::Char('$'), true, false) => Some(Action::MoveToLineEnd),
		(KeyCode::Char('d'), true, false) => Some(Action::CutWord),
		(KeyCode::Char('y'), true, false) => Some(Action::Copy),
		(KeyCode::Char('p'), true, false) => Some(Action::Paste),

		(KeyCode::Char('b') | KeyCode::Char('B'), true, true) => Some(Action::SelectWordLeft),
		(KeyCode::Char('w') | KeyCode::Char('W'), true, true) => Some(Action::SelectWordRight),

		(KeyCode::Left, false, false) => Some(Action::MoveLeft),
		(KeyCode::Right, false, false) => Some(Action::MoveRight),
		(KeyCode::Home, false, false) => Some(Action::MoveToLineStart),
		(KeyCode::End, false, false) => Some(Action::MoveToLineEnd),
		(KeyCode::Backspace, false, false) => Some(Action::Backspace),
		(KeyCode::Delete, false, false) => Some(Action::Delete),
		(KeyCode::Esc, false, false) => Some(Action::CancelForm),
		(KeyCode::Enter, false, false) => Some(Action::SubmitForm),
		(KeyCode::Tab, false, false) => Some(Action::NextField),

		_ => None,
	}
}

fn vscode_create_project_action(key: KeyEvent) -> Option<Action> {
	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	let shift = key.modifiers.contains(KeyModifiers::SHIFT);

	match (key.code, ctrl, shift) {
		(KeyCode::Char('c'), true, false) => Some(Action::Copy),
		(KeyCode::Char('x'), true, false) => Some(Action::Cut),
		(KeyCode::Char('v'), true, false) => Some(Action::Paste),
		(KeyCode::Char('a'), true, false) => Some(Action::SelectAll),

		(KeyCode::Left, true, false) => Some(Action::MoveWordLeft),
		(KeyCode::Right, true, false) => Some(Action::MoveWordRight),
		(KeyCode::Left, true, true) => Some(Action::SelectWordLeft),
		(KeyCode::Right, true, true) => Some(Action::SelectWordRight),

		(KeyCode::Left, false, false) => Some(Action::MoveLeft),
		(KeyCode::Right, false, false) => Some(Action::MoveRight),
		(KeyCode::Home, false, false) => Some(Action::MoveToLineStart),
		(KeyCode::End, false, false) => Some(Action::MoveToLineEnd),
		(KeyCode::Backspace, false, false) => Some(Action::Backspace),
		(KeyCode::Delete, false, false) => Some(Action::Delete),
		(KeyCode::Esc, false, false) => Some(Action::CancelForm),
		(KeyCode::Enter, false, false) => Some(Action::SubmitForm),
		(KeyCode::Tab, false, false) => Some(Action::NextField),

		_ => None,
	}
}

fn buffer_view_action(key: KeyEvent, keymap: Keymap) -> Option<Action> {
	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	let alt = key.modifiers.contains(KeyModifiers::ALT);

	// Клавиши уровня проекта работают и внутри буфера
	if let Some(action) = tab_switch_action(key) {
		return Some(action);
	}

	if !ctrl && !alt {
		match key.code {
			KeyCode::Esc => return Some(Action::ExitProject),
			KeyCode::F(5) => return Some(Action::RestartProject),
			_ => {}
		}
	}
	if ctrl && !alt {
		match key.code {
			KeyCode::Char('r') => return Some(Action::RestartProject),
			_ => {}
		}
	}

	// Универсальная навигация и выделение
	let shift = key.modifiers.contains(KeyModifiers::SHIFT);
	match (key.code, ctrl, alt, shift) {
		(KeyCode::Up, false, false, false) => return Some(Action::BufferMoveUp),
		(KeyCode::Down, false, false, false) => return Some(Action::BufferMoveDown),
		(KeyCode::Left, false, false, false) => return Some(Action::BufferMoveLeft),
		(KeyCode::Right, false, false, false) => return Some(Action::BufferMoveRight),
		(KeyCode::Up, false, false, true) => return Some(Action::BufferSelectUp),
		(KeyCode::Down, false, false, true) => return Some(Action::BufferSelectDown),
		(KeyCode::Left, false, false, true) => return Some(Action::BufferSelectLeft),
		(KeyCode::Right, false, false, true) => return Some(Action::BufferSelectRight),
		(KeyCode::PageUp, false, false, _) => return Some(Action::BufferPageUp),
		(KeyCode::PageDown, false, false, _) => return Some(Action::BufferPageDown),
		_ => {}
	}

	match keymap {
		Keymap::Emacs => emacs_buffer_view_action(key),
		Keymap::Vim => vim_buffer_view_action(key),
		Keymap::Vscode => vscode_buffer_view_action(key),
	}
}

fn emacs_buffer_view_action(key: KeyEvent) -> Option<Action> {
	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	let alt = key.modifiers.contains(KeyModifiers::ALT);

	match (key.code, ctrl, alt) {
		(KeyCode::Char(' '), true, false) => Some(Action::BufferToggleMark),
		(KeyCode::Char('<'), false, true) => Some(Action::BufferHome),
		(KeyCode::Char('>'), false, true) => Some(Action::BufferEnd),
		(KeyCode::Char('w'), false, true) => Some(Action::BufferCopy),
		(KeyCode::Home, true, false) => Some(Action::BufferHome),
		(KeyCode::End, true, false) => Some(Action::BufferEnd),
		(KeyCode::Char('a'), true, false) => Some(Action::BufferSelectAll),
		_ => None,
	}
}

fn vim_buffer_view_action(key: KeyEvent) -> Option<Action> {
	let alt = key.modifiers.contains(KeyModifiers::ALT);
	let shift = key.modifiers.contains(KeyModifiers::SHIFT);

	match (key.code, alt, shift) {
		(KeyCode::Char('h'), true, false) => Some(Action::BufferMoveLeft),
		(KeyCode::Char('j'), true, false) => Some(Action::BufferMoveDown),
		(KeyCode::Char('k'), true, false) => Some(Action::BufferMoveUp),
		(KeyCode::Char('l'), true, false) => Some(Action::BufferMoveRight),
		(KeyCode::Char('h') | KeyCode::Char('H'), true, true) => Some(Action::BufferSelectLeft),
		(KeyCode::Char('j') | KeyCode::Char('J'), true, true) => Some(Action::BufferSelectDown),
		(KeyCode::Char('k') | KeyCode::Char('K'), true, true) => Some(Action::BufferSelectUp),
		(KeyCode::Char('l') | KeyCode::Char('L'), true, true) => Some(Action::BufferSelectRight),
		(KeyCode::Char('y'), true, false) => Some(Action::BufferCopy),
		(KeyCode::Char('g'), true, false) => Some(Action::BufferEnd),
		_ => None,
	}
}

fn vscode_buffer_view_action(key: KeyEvent) -> Option<Action> {
	let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
	let shift = key.modifiers.contains(KeyModifiers::SHIFT);

	match (key.code, ctrl, shift) {
		(KeyCode::Char('c'), true, false) => Some(Action::BufferCopy),
		(KeyCode::Char('a'), true, false) => Some(Action::BufferSelectAll),
		(KeyCode::Home, true, false) => Some(Action::BufferHome),
		(KeyCode::End, true, false) => Some(Action::BufferEnd),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_projects_navigation() {
		let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
		assert_eq!(
			key_to_action(key, KeyContext::Projects, Keymap::Emacs),
			Some(Action::NavigateUp)
		);
	}

	#[test]
	fn test_projects_enter_search() {
		let key = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
		assert_eq!(
			key_to_action(key, KeyContext::Projects, Keymap::Emacs),
			Some(Action::EnterSearch)
		);
	}

	#[test]
	fn test_project_search_input() {
		let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
		assert_eq!(
			key_to_action(key, KeyContext::ProjectSearch, Keymap::Emacs),
			Some(Action::SearchChar('q'))
		);

		let key = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
		assert_eq!(
			key_to_action(key, KeyContext::ProjectSearch, Keymap::Emacs),
			Some(Action::SearchBackspace)
		);
	}

	#[test]
	fn test_project_search_control() {
		let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
		assert_eq!(
			key_to_action(key, KeyContext::ProjectSearch, Keymap::Emacs),
			Some(Action::ExitSearch)
		);

		let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
		assert_eq!(
			key_to_action(key, KeyContext::ProjectSearch, Keymap::Emacs),
			Some(Action::SearchSubmit)
		);
	}

	#[test]
	fn test_confirm_delete_keys() {
		let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
		assert_eq!(
			key_to_action(key, KeyContext::ConfirmDelete, Keymap::Emacs),
			Some(Action::ConfirmDelete)
		);

		let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
		assert_eq!(
			key_to_action(key, KeyContext::ConfirmDelete, Keymap::Emacs),
			Some(Action::CancelDelete)
		);
	}

	#[test]
	fn test_buffer_view_tab_switching() {
		let cases = [
			(KeyCode::Char('C'), Action::SwitchToConsole),
			(KeyCode::Char('N'), Action::SwitchToNetwork),
			(KeyCode::Char('R'), Action::SwitchToResources),
			(KeyCode::Char('D'), Action::SwitchToDom),
			(KeyCode::Char('L'), Action::SwitchToLogs),
		];

		for (code, expected) in cases {
			let key = KeyEvent::new(code, KeyModifiers::SHIFT);
			for keymap in [Keymap::Emacs, Keymap::Vim, Keymap::Vscode] {
				assert_eq!(
					key_to_action(key, KeyContext::BufferView, keymap),
					Some(expected.clone()),
					"tab switch failed for {:?} in {:?}",
					code,
					keymap
				);
			}
		}
	}

	#[test]
	fn test_buffer_view_esc_exits_project() {
		let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
		for keymap in [Keymap::Emacs, Keymap::Vim, Keymap::Vscode] {
			assert_eq!(
				key_to_action(key, KeyContext::BufferView, keymap),
				Some(Action::ExitProject)
			);
		}
	}

	#[test]
	fn test_f5_restarts_project() {
		let key = KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE);
		assert_eq!(
			key_to_action(key, KeyContext::ProjectTabs, Keymap::Emacs),
			Some(Action::RestartProject)
		);
		assert_eq!(
			key_to_action(key, KeyContext::BufferView, Keymap::Emacs),
			Some(Action::RestartProject)
		);
	}

	#[test]
	fn test_ctrl_r_restarts_project() {
		let key = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL);
		assert_eq!(
			key_to_action(key, KeyContext::ProjectTabs, Keymap::Emacs),
			Some(Action::RestartProject)
		);
		assert_eq!(
			key_to_action(key, KeyContext::BufferView, Keymap::Emacs),
			Some(Action::RestartProject)
		);
	}

	#[test]
	fn test_shift_v_toggles_visibility() {
		let key = KeyEvent::new(KeyCode::Char('V'), KeyModifiers::SHIFT);
		assert_eq!(
			key_to_action(key, KeyContext::ProjectTabs, Keymap::Emacs),
			Some(Action::ToggleTabVisibility)
		);
		assert_eq!(
			key_to_action(key, KeyContext::BufferView, Keymap::Emacs),
			Some(Action::ToggleTabVisibility)
		);
	}

	#[test]
	fn test_create_project_q_is_text() {
		let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
		assert_eq!(
			key_to_action(key, KeyContext::CreateProject, Keymap::Emacs),
			Some(Action::InputChar('q'))
		);
	}

	#[test]
	fn test_create_project_emacs() {
		let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
		assert_eq!(
			key_to_action(key, KeyContext::CreateProject, Keymap::Emacs),
			Some(Action::MoveToLineStart)
		);

		let key = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL);
		assert_eq!(
			key_to_action(key, KeyContext::CreateProject, Keymap::Emacs),
			Some(Action::CutWord)
		);
	}

	#[test]
	fn test_create_project_vscode() {
		let key = KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
		assert_eq!(
			key_to_action(key, KeyContext::CreateProject, Keymap::Vscode),
			Some(Action::SelectWordRight)
		);
	}

	#[test]
	fn test_buffer_view_emacs() {
		let key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL);
		assert_eq!(
			key_to_action(key, KeyContext::BufferView, Keymap::Emacs),
			Some(Action::BufferToggleMark)
		);

		let key = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::ALT);
		assert_eq!(
			key_to_action(key, KeyContext::BufferView, Keymap::Emacs),
			Some(Action::BufferCopy)
		);
	}

	#[test]
	fn test_buffer_view_vscode() {
		let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
		assert_eq!(
			key_to_action(key, KeyContext::BufferView, Keymap::Vscode),
			Some(Action::BufferCopy)
		);
	}

	#[test]
	fn test_buffer_view_universal() {
		let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
		assert_eq!(
			key_to_action(key, KeyContext::BufferView, Keymap::Emacs),
			Some(Action::BufferMoveUp)
		);

		let key = KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT);
		assert_eq!(
			key_to_action(key, KeyContext::BufferView, Keymap::Emacs),
			Some(Action::BufferSelectUp)
		);
	}

	#[test]
	fn test_toggle_keymap() {
		let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
		assert_eq!(
			key_to_action(key, KeyContext::Global, Keymap::Emacs),
			Some(Action::ToggleKeymap)
		);
	}
}
