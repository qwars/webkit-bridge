use anyhow::Result;
use ratatui::{
	layout::Rect,
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, List, ListItem, Paragraph},
	Frame,
};

use crate::app::AppState;
use crate::keys::{Action, Keymap};
use crate::views::View;

pub struct ProjectsView;

impl ProjectsView {
	/// Строка поиска с курсором
	fn search_line(query: &str) -> Line<'static> {
		Line::from(vec![
			Span::styled("/ ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
			Span::styled(query.to_string(), Style::default().fg(Color::White)),
			Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)),
		])
	}

	/// Подсветка совпавшей подстроки в имени проекта
	fn name_spans(name: &str, query: &str, base_style: Style) -> Vec<Span<'static>> {
		if query.is_empty() {
			return vec![Span::styled(name.to_string(), base_style)];
		}

		let name_lower: Vec<char> = name.to_lowercase().chars().collect();
		let query_lower: Vec<char> = query.to_lowercase().chars().collect();
		let name_chars: Vec<char> = name.chars().collect();

		let matched_at = name_lower
			.windows(query_lower.len())
			.position(|window| window == query_lower.as_slice());

		match matched_at {
			Some(start) => {
				let before: String = name_chars[..start].iter().collect();
				let matched: String = name_chars[start..start + query_lower.len()].iter().collect();
				let after: String = name_chars[start + query_lower.len()..].iter().collect();

				vec![
					Span::styled(before, base_style),
					Span::styled(
						matched,
						base_style
							.fg(Color::Yellow)
							.add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
					),
					Span::styled(after, base_style),
				]
			}
			None => vec![Span::styled(name.to_string(), base_style)],
		}
	}
}

impl View for ProjectsView {
	fn draw(&self, f: &mut Frame, state: &AppState) {
		let area = f.area();

		let title = Block::default()
			.borders(Borders::ALL)
			.title(format!(" Проекты ({}) ", state.projects.len()))
			.title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

		let inner = title.inner(area);
		f.render_widget(title, area);

		// В режиме поиска верхняя строка внутренней области — поле ввода
		let (search_area, list_area) = if state.search_mode && inner.height > 1 {
			(
				Some(Rect {
					x: inner.x,
					y: inner.y,
					width: inner.width,
					height: 1,
				}),
				Rect {
					x: inner.x,
					y: inner.y + 1,
					width: inner.width,
					height: inner.height - 1,
				},
			)
		} else {
			(None, inner)
		};

		if let Some(search_area) = search_area {
			f.render_widget(Paragraph::new(Self::search_line(&state.search_query)), search_area);
		}

		let visible = state.filtered_projects();
		if visible.is_empty() {
			let text = if state.search_mode {
				"Ничего не найдено"
			} else {
				"Нет проектов. Нажмите 'n' для создания."
			};
			let msg = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
			f.render_widget(msg, list_area);
		} else {
			let items: Vec<ListItem> = visible
				.iter()
				.enumerate()
				.map(|(i, project)| {
					let is_selected = i == state.selected_index;
					let style = if is_selected {
						Style::default()
							.fg(Color::White)
							.bg(Color::DarkGray)
							.add_modifier(Modifier::BOLD)
					} else {
						Style::default().fg(Color::White)
					};

					// Индикатор *: источники новее бандла (или бандл отсутствует)
					let needs_rebuild = match crate::projects::get_bundle_mtime(&project.project.dir) {
						Ok(bundle_mtime) => project.mtime > bundle_mtime,
						Err(_) => true, // бандл отсутствует — нужна сборка
					};

					let mut spans = Vec::new();
					if needs_rebuild {
						spans.push(Span::styled(
							"* ",
							Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
						));
					}

					if state.search_mode {
						spans.extend(Self::name_spans(
							&project.project.config.name,
							&state.search_query,
							style,
						));
					} else {
						spans.push(Span::styled(project.project.config.name.clone(), style));
					}

					spans.push(Span::raw("  "));
					spans.push(Span::styled(
						project.project.config.url.clone(),
						Style::default().fg(Color::DarkGray),
					));

					spans.push(Span::raw("  "));
					spans.push(Span::styled(
						format!("[{}]", project.project.dir),
						Style::default().fg(Color::Cyan),
					));

					spans.push(Span::raw("  "));
					spans.push(Span::styled(
						crate::projects::format_mtime(project.mtime),
						Style::default().fg(Color::DarkGray),
					));

					ListItem::new(Line::from(spans))
				})
				.collect();

			f.render_widget(List::new(items), list_area);
		}

		let hints_area = Rect {
			x: area.x,
			y: area.height.saturating_sub(1),
			width: area.width,
			height: 1,
		};

		let key_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);

		let keymap_label = match state.keymap {
			Keymap::Emacs => "Emacs",
			Keymap::Vim => "Vim",
			Keymap::Vscode => "VSCode",
		};
		let keymap_hint = format!(" ({})  ", keymap_label);

		let hints = if state.search_mode {
			Paragraph::new(Line::from(vec![
				Span::raw(" "),
				Span::styled("↑/↓", key_style),
				Span::raw(" выбор  "),
				Span::styled("Enter", key_style),
				Span::raw(" открыть  "),
				Span::styled("Esc", key_style),
				Span::raw(" отмена поиска"),
			]))
		} else {
			Paragraph::new(Line::from(vec![
				Span::raw(" "),
				Span::styled("↑/↓", key_style),
				Span::raw(" навигация  "),
				Span::styled("Enter", key_style),
				Span::raw(" войти  "),
				Span::styled("/", key_style),
				Span::raw(" поиск  "),
				Span::styled("n", key_style),
				Span::raw(" создать  "),
				Span::styled("d", key_style),
				Span::raw(" удалить  "),
				Span::styled("Ctrl+K", key_style),
				Span::raw(keymap_hint.as_str()),
				Span::styled("q", key_style),
				Span::raw(" выход"),
			]))
		};

		f.render_widget(hints, hints_area);
	}

	async fn handle_action(&mut self, action: Action, state: &mut AppState) -> Result<bool> {
		match action {
			Action::NavigateUp => {
				state.navigate_up();
				Ok(true)
			}
			Action::NavigateDown => {
				state.navigate_down();
				Ok(true)
			}
			Action::EnterProject => {
				state.enter_project();
				Ok(true)
			}
			Action::NewProject => {
				state.start_create_project();
				Ok(true)
			}
			Action::DeleteProject => {
				state.request_delete();
				Ok(true)
			}

			Action::EnterSearch => {
				state.enter_search();
				Ok(true)
			}
			Action::ExitSearch => {
				state.exit_search();
				Ok(true)
			}
			Action::SearchChar(c) => {
				state.search_input_char(c);
				Ok(true)
			}
			Action::SearchBackspace => {
				state.search_backspace();
				Ok(true)
			}
			Action::SearchSubmit => {
				state.search_submit();
				Ok(true)
			}
			_ => Ok(false),
		}
	}
}
