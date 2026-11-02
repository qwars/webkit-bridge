use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use ratatui::DefaultTerminal;

use crate::app::{AppState, BufferKind, Screen};
use crate::keys::{key_to_action, Action, KeyContext};
use crate::views::confirm_delete::ConfirmDeleteView;
use crate::views::create_project::CreateProjectView;
use crate::views::project::{ProjectTab, ProjectView};
use crate::views::projects::ProjectsView;
use crate::views::View;

pub async fn run_ui(mut state: AppState) -> Result<()> {
	let mut terminal = ratatui::init();
	let result = run_app(&mut terminal, &mut state).await;
	ratatui::restore();
	result
}

async fn run_app(terminal: &mut DefaultTerminal, state: &mut AppState) -> Result<()> {
	let mut projects_view = ProjectsView;
	let mut project_view = ProjectView::new();
	let mut create_project_view = CreateProjectView;
	let mut confirm_delete_view = ConfirmDeleteView;

	loop {
		state.drain_logs();
		state.drain_watcher_events().await;
		state.drain_build_results();

		// Если проект ожидает запуска и сборка завершена — запускаем
		if let Some(dir) = state.pending_start.take() {
			if !state.is_building {
				if let Err(e) = state.start_project(&dir).await {
					log::error!("Failed to start project '{}': {}", dir, e);
				}
			} else {
				// Сборка ещё идёт — возвращаем pending_start
				state.pending_start = Some(dir);
			}
		}

		terminal.draw(|f| {
			draw_ui(
				f,
				state,
				&mut projects_view,
				&mut project_view,
				&mut create_project_view,
				&mut confirm_delete_view,
			);
		})?;

		if event::poll(std::time::Duration::from_millis(100))? {
			if let Event::Key(key) = event::read()? {
				if key.kind == crossterm::event::KeyEventKind::Release {
					continue;
				}

				handle_key_event(
					key,
					state,
					&mut projects_view,
					&mut project_view,
					&mut create_project_view,
					&mut confirm_delete_view,
				)
				.await?;

				if state.should_quit {
					break;
				}
			}
		}
	}

	Ok(())
}

fn draw_ui(
	f: &mut ratatui::Frame,
	state: &AppState,
	projects_view: &mut ProjectsView,
	project_view: &mut ProjectView,
	create_project_view: &mut CreateProjectView,
	confirm_delete_view: &mut ConfirmDeleteView,
) {
	match &state.screen {
		Screen::Projects => {
			projects_view.draw(f, state);
		}
		Screen::Project(_) => {
			project_view.draw(f, state);
		}
		Screen::CreateProject => {
			create_project_view.draw(f, state);
		}
		Screen::ConfirmDelete => {
			confirm_delete_view.draw(f, state);
		}
	}
}

async fn handle_key_event(
	key: KeyEvent,
	state: &mut AppState,
	projects_view: &mut ProjectsView,
	project_view: &mut ProjectView,
	create_project_view: &mut CreateProjectView,
	confirm_delete_view: &mut ConfirmDeleteView,
) -> Result<()> {
	let context = match &state.screen {
		Screen::Projects => {
			if state.search_mode {
				KeyContext::ProjectSearch
			} else {
				KeyContext::Projects
			}
		}
		Screen::Project(_) => match project_view.active_tab() {
			ProjectTab::Console => {
				state.active_buffer = BufferKind::Console;
				KeyContext::BufferView
			}
			ProjectTab::Logs => {
				state.active_buffer = BufferKind::Logs;
				KeyContext::BufferView
			}
			_ => KeyContext::ProjectTabs,
		},
		Screen::CreateProject => KeyContext::CreateProject,
		Screen::ConfirmDelete => KeyContext::ConfirmDelete,
	};

	// Блокируем все клавиши во время сборки
	if state.is_building {
		if let Some(action) = key_to_action(key, context, state.keymap) {
			if matches!(action, Action::ToggleTabVisibility) {
				// Разрешаем переключение видимости даже во время сборки
				if let Screen::Project(_) = &state.screen {
					project_view.handle_action(action, state).await?;
				}
			}
		}
		return Ok(());
	}

	let text_input = matches!(context, KeyContext::CreateProject | KeyContext::ProjectSearch);

	if !text_input {
		if let Some(action) = key_to_action(key, KeyContext::Global, state.keymap) {
			match action {
				Action::Quit => {
					state.should_quit = true;
					return Ok(());
				}
				Action::ToggleKeymap => {
					state.toggle_keymap()?;
					return Ok(());
				}
				_ => {}
			}
		}
	}

	if let Some(action) = key_to_action(key, context, state.keymap) {
		match &mut state.screen {
			Screen::Projects => {
				projects_view.handle_action(action, state).await?;
			}
			Screen::Project(_) => {
				project_view.handle_action(action, state).await?;
			}
			Screen::CreateProject => {
				create_project_view.handle_action(action, state).await?;
			}
			Screen::ConfirmDelete => {
				confirm_delete_view.handle_action(action, state).await?;
			}
		}
	}

	Ok(())
}
