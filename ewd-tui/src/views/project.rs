use anyhow::Result;
use ratatui::{
	layout::{Alignment, Rect},
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Padding, Paragraph},
	Frame,
};

use crate::app::{AppState, ProjectStatus};
use crate::keys::Action;
use crate::views::View;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTab {
	Console,
	Network,
	Resources,
	Dom,
	Logs,
}

impl ProjectTab {
	fn as_str(&self) -> &'static str {
		match self {
			ProjectTab::Console => "Console",
			ProjectTab::Network => "Network",
			ProjectTab::Resources => "Resources",
			ProjectTab::Dom => "DOM",
			ProjectTab::Logs => "Logs",
		}
	}

	fn shortcut(&self) -> &'static str {
		match self {
			ProjectTab::Console => "C",
			ProjectTab::Network => "N",
			ProjectTab::Resources => "R",
			ProjectTab::Dom => "D",
			ProjectTab::Logs => "L",
		}
	}
}

pub struct ProjectView {
	active_tab: ProjectTab,
}

impl ProjectView {
	pub fn new() -> Self {
		Self {
			active_tab: ProjectTab::Console,
		}
	}
	pub fn active_tab(&self) -> ProjectTab {
		self.active_tab
	}
	fn draw_text_buffer(&self, f: &mut Frame, area: Rect, state: &AppState) {
		let buffer = match self.active_tab {
			ProjectTab::Console => &state.console_buffer,
			ProjectTab::Logs => &state.log_buffer,
			_ => return,
		};

		if buffer.lines.is_empty() {
			let msg = Paragraph::new("Пусто").style(Style::default().fg(Color::DarkGray));
			f.render_widget(msg, area);
			return;
		}

		let area_height = area.height as usize;
		let mut buffer_clone = buffer.clone();
		buffer_clone.update_scroll(area_height);

		let (start, end) = buffer_clone.visible_lines(area_height);
		let visible_lines: Vec<Line> = buffer_clone.lines[start..end]
			.iter()
			.enumerate()
			.map(|(i, buffer_line)| {
				let row = start + i;
				let mut spans = Vec::new();
				let mut current_col = 0;
				let selection = buffer_clone.selection_range();

				for span in &buffer_line.line.spans {
					for ch in span.content.chars() {
						let is_cursor = row == buffer_clone.cursor_row && current_col == buffer_clone.cursor_col;
						let is_selected = selection.map_or(false, |(sr, sc, er, ec)| {
							if row == sr && row == er {
								current_col >= sc && current_col < ec
							} else if row == sr {
								current_col >= sc
							} else if row == er {
								current_col < ec
							} else {
								row > sr && row < er
							}
						});

						let mut style = span.style;
						if is_cursor || is_selected {
							style = style.add_modifier(Modifier::REVERSED);
						}

						spans.push(Span::styled(ch.to_string(), style));
						current_col += 1;
					}
				}

				if buffer_clone.cursor_row == row
					&& buffer_clone.cursor_col == buffer_line.text.len()
					&& !buffer_clone.has_selection()
				{
					spans.push(Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)));
				}

				Line::from(spans)
			})
			.collect();

		f.render_widget(Paragraph::new(visible_lines), area);
	}
}

impl View for ProjectView {
	fn draw(&self, f: &mut Frame, state: &AppState) {
		let area = f.area();
		let project = match state.current_project() {
			Some(p) => p,
			None => return,
		};

		let left_title = Line::from(vec![Span::styled(
			format!(" {} ", project.project.config.name),
			Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
		)]);

		let tabs: Vec<Span> = [
			ProjectTab::Console,
			ProjectTab::Network,
			ProjectTab::Resources,
			ProjectTab::Dom,
			ProjectTab::Logs,
		]
		.iter()
		.map(|tab| {
			let style = if *tab == self.active_tab {
				Style::default()
					.fg(Color::Black)
					.bg(Color::White)
					.add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::DarkGray)
			};
			Span::styled(format!("[{}] {} ", tab.shortcut(), tab.as_str()), style)
		})
		.collect();

		let block = Block::default()
			.borders(Borders::ALL)
			.title_top(left_title.alignment(Alignment::Left))
			.title_top(Line::from(tabs).alignment(Alignment::Right))
			.padding(Padding::uniform(1));

		let inner = block.inner(area); // Сначала берём inner
		f.render_widget(block, area); // Потом перемещаем block в render_widget

		match self.active_tab {
			ProjectTab::Console | ProjectTab::Logs => self.draw_text_buffer(f, inner, state),
			other => {
				let msg =
					Paragraph::new(format!("{}: заглушка", other.as_str())).style(Style::default().fg(Color::DarkGray));
				f.render_widget(msg, inner);
			}
		}

		let hints_area = Rect {
			x: area.x,
			y: area.height.saturating_sub(1),
			width: area.width,
			height: 1,
		};
		let key_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);

		let has_watcher = state.active_watchers.contains_key(&project.project.dir);
		let visibility = if project.tab_visible {
			Span::styled(" [visible]", Style::default().fg(Color::Cyan))
		} else {
			Span::styled(" [headless]", Style::default().fg(Color::DarkGray))
		};
		let watch = if has_watcher {
			Span::styled(" [watch]", Style::default().fg(Color::Green))
		} else {
			Span::raw("")
		};
		let building = if state.is_building {
			Span::styled(" [building...]", Style::default().fg(Color::Yellow))
		} else {
			Span::raw("")
		};

		let left_hints = Line::from(vec![
			Span::raw(" "),
			Span::styled("Shift+C/N/R/D/L", key_style),
			Span::raw(" вкладки  "),
			Span::styled("F5", key_style),
			Span::raw(" сборка  "),
			Span::styled("Shift+V", key_style),
			Span::raw(" видимость  "),
			Span::styled("Esc", key_style),
			Span::raw(" выход"),
		]);
		let right_hints = Line::from(vec![visibility, watch, building]);

		let chunks = ratatui::layout::Layout::default()
			.direction(ratatui::layout::Direction::Horizontal)
			.constraints([
				ratatui::layout::Constraint::Fill(1),
				ratatui::layout::Constraint::Length(40),
			])
			.split(hints_area);

		f.render_widget(Paragraph::new(left_hints).alignment(Alignment::Left), chunks[0]);
		f.render_widget(Paragraph::new(right_hints).alignment(Alignment::Right), chunks[1]);
	}

	async fn handle_action(&mut self, action: Action, state: &mut AppState) -> Result<bool> {
		match action {
			Action::SwitchToConsole => {
				self.active_tab = ProjectTab::Console;
				state.active_buffer = crate::app::BufferKind::Console;
				Ok(true)
			}
			Action::SwitchToNetwork => {
				self.active_tab = ProjectTab::Network;
				Ok(true)
			}
			Action::SwitchToResources => {
				self.active_tab = ProjectTab::Resources;
				Ok(true)
			}
			Action::SwitchToDom => {
				self.active_tab = ProjectTab::Dom;
				Ok(true)
			}
			Action::SwitchToLogs => {
				self.active_tab = ProjectTab::Logs;
				state.active_buffer = crate::app::BufferKind::Logs;
				Ok(true)
			}
			Action::ExitProject => {
				state.stop_and_exit_project().await?;
				Ok(true)
			}
			Action::RestartProject => {
				if let Some(proj) = state.current_project().cloned() {
					let dir = proj.project.dir.clone();
					let was_running = proj.status == ProjectStatus::Running;
					if let Err(e) = state.start_build(&dir) {
						log::error!("{}", e);
						self.active_tab = ProjectTab::Console;
						return Ok(true);
					}
					if !was_running {
						state.start_project(&dir).await?;
					}
				}
				Ok(true)
			}
			Action::ToggleTabVisibility => {
				if let Some(proj) = state.current_project().cloned() {
					state.toggle_tab_visibility(&proj.project.dir).await?;
				}
				Ok(true)
			}
			Action::BufferMoveUp => {
				state.buffer_move_up();
				Ok(true)
			}
			Action::BufferMoveDown => {
				state.buffer_move_down();
				Ok(true)
			}
			Action::BufferMoveLeft => {
				state.buffer_move_left();
				Ok(true)
			}
			Action::BufferMoveRight => {
				state.buffer_move_right();
				Ok(true)
			}
			Action::BufferPageUp => {
				state.buffer_page_up(10);
				Ok(true)
			}
			Action::BufferPageDown => {
				state.buffer_page_down(10);
				Ok(true)
			}
			Action::BufferHome => {
				state.buffer_home();
				Ok(true)
			}
			Action::BufferEnd => {
				state.buffer_end();
				Ok(true)
			}
			Action::BufferSelectUp => {
				state.buffer_select_up();
				Ok(true)
			}
			Action::BufferSelectDown => {
				state.buffer_select_down();
				Ok(true)
			}
			Action::BufferSelectLeft => {
				state.buffer_select_left();
				Ok(true)
			}
			Action::BufferSelectRight => {
				state.buffer_select_right();
				Ok(true)
			}
			Action::BufferSelectAll => {
				state.buffer_select_all();
				Ok(true)
			}
			Action::BufferToggleMark => {
				state.buffer_toggle_mark();
				Ok(true)
			}
			Action::BufferCopy => {
				state.buffer_copy();
				Ok(true)
			}
			_ => Ok(false),
		}
	}
}
