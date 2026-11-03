use crate::app::{AppState, ProjectStatus};
use crate::keys::Action;
use crate::views::View;
use anyhow::Result;
use ratatui::{
	layout::{Alignment, Rect},
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Padding, Paragraph},
	Frame,
};

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
			ProjectTab::Console => "Console   ",
			ProjectTab::Network => "Network   ",
			ProjectTab::Resources => "Resources   ",
			ProjectTab::Dom => "DOM   ",
			ProjectTab::Logs => "Logs   ",
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

	fn format_bytes(bytes: u64) -> String {
		if bytes >= 1_000_000 {
			format!("{:.1}Mb", bytes as f64 / 1_000_000.0)
		} else if bytes >= 1_000 {
			format!("{:.1}Kb", bytes as f64 / 1_000.0)
		} else {
			format!("{}b", bytes)
		}
	}

	fn draw_text_buffer(&self, f: &mut Frame, area: Rect, state: &AppState) {
		if self.active_tab == ProjectTab::Network {
			self.draw_network_buffer(f, area, state);
			return;
		}

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
						let is_selected = selection.is_some_and(|(sr, sc, er, ec)| {
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

	fn draw_network_buffer(&self, f: &mut Frame, area: Rect, state: &AppState) {
		let visible_indices = state.visible_network_indices();
		if visible_indices.is_empty() {
			let msg = if state.network_events.is_empty() {
				"Пусто"
			} else {
				"Нет совпадений"
			};
			let msg_widget = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
			f.render_widget(msg_widget, area);
			return;
		}

		let area_height = area.height as usize;
		let cursor = state.network_cursor.min(visible_indices.len().saturating_sub(1));

		let scroll_offset = if cursor < state.network_buffer.scroll_offset {
			cursor
		} else if cursor >= state.network_buffer.scroll_offset + area_height {
			cursor - area_height + 1
		} else {
			state.network_buffer.scroll_offset
		};

		let start = scroll_offset;
		let end = (start + area_height).min(visible_indices.len());
		let mut visible_lines = Vec::new();

		let mark_len = 2;

		for (i, &_event_idx) in visible_indices.iter().enumerate().skip(start).take(end - start) {
			let event_idx = visible_indices[i];
			let event = &state.network_events[event_idx];

			let method_upper = event.method.to_uppercase();
			let status_str = event
				.status_code
				.map(|s| s.to_string())
				.unwrap_or_else(|| "-".to_string());
			let time_str = event
				.response_time_ms
				.map(|t| format!("{}ms", t))
				.unwrap_or_else(|| "-".to_string());
			let size_str = event
				.response_size_bytes
				.map(Self::format_bytes)
				.unwrap_or_else(|| "-".to_string());

			let left_part = format!("[ {} {} ] - ", method_upper, status_str);
			let right_part = format!("[{}] [{}]", time_str, size_str);

			let left_len = left_part.len();
			let right_len = right_part.len();

			let min_required_width = mark_len + left_len + right_len + 1;
			let available_for_url = (area.width as usize).saturating_sub(min_required_width);

			let display_url = if event.url.len() > available_for_url {
				if available_for_url > 3 {
					format!("{}...", &event.url[..available_for_url - 3])
				} else if available_for_url > 0 {
					event.url[..available_for_url].to_string()
				} else {
					String::new()
				}
			} else {
				event.url.clone()
			};

			let padding_len =
				(area.width as usize).saturating_sub(mark_len + left_len + display_url.len() + right_len + 1);
			let padding = " ".repeat(padding_len);

			let text = format!("{}{}{} {}", left_part, display_url, padding, right_part);

			let is_error = matches!(event.event_type, ewd_core::events::NetworkEventType::Failed)
				|| event.status_code.is_some_and(|s| s >= 400);

			let color = if is_error {
				Color::Red
			} else if matches!(event.event_type, ewd_core::events::NetworkEventType::Started) {
				Color::DarkGray
			} else {
				match method_upper.as_str() {
					"GET" => Color::Green,
					"POST" => Color::Cyan,
					"PUT" => Color::Yellow,
					"DELETE" => Color::Magenta,
					"PATCH" => Color::LightYellow,
					"OPTIONS" | "HEAD" => Color::DarkGray,
					_ => Color::White,
				}
			};

			let is_marked = state.marked_network_indices.contains(&event_idx);
			let mark_span = if is_marked {
				Span::styled("✓ ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
			} else {
				Span::raw("  ")
			};

			let is_selected = i == cursor;
			let style = if is_selected {
				Style::default()
					.fg(color)
					.bg(Color::DarkGray)
					.add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(color)
			};

			visible_lines.push(Line::from(vec![mark_span, Span::styled(text, style)]));
		}

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

		let inner = block.inner(area);
		f.render_widget(block, area);

		match self.active_tab {
			ProjectTab::Console | ProjectTab::Logs | ProjectTab::Network => self.draw_text_buffer(f, inner, state),
			other => {
				let msg =
					Paragraph::new(format!("{}: заглушка", other.as_str())).style(Style::default().fg(Color::DarkGray));
				f.render_widget(msg, inner);
			}
		}

		if self.active_tab == ProjectTab::Network && state.network_search_mode {
			let search_area = Rect {
				x: area.x,
				y: area.height.saturating_sub(1),
				width: area.width,
				height: 1,
			};
			let filtered_count = state.visible_network_indices().len();
			let total_count = state.network_events.len();
			let search_text = format!(" /{} │ {}/{} ", state.network_search_query, filtered_count, total_count);
			let search_widget =
				Paragraph::new(search_text).style(Style::default().fg(Color::Yellow).bg(Color::DarkGray));
			f.render_widget(search_widget, search_area);
			return;
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

		let left_hints = if self.active_tab == ProjectTab::Network {
			let marked_count = state.marked_network_indices.len();
			let total_count = state.network_events.len();
			let marks_counter = Span::styled(
				format!(" [Отмечено: {}/{}] ", marked_count, total_count),
				Style::default().fg(Color::Yellow),
			);
			Line::from(vec![
				Span::raw(" "),
				Span::styled("↑/↓", key_style),
				Span::raw(" выбор  "),
				Span::styled("Enter", key_style),
				Span::raw(" inspect  "),
				Span::styled("Space", key_style),
				Span::raw(" отметка  "),
				Span::styled("c", key_style),
				Span::raw(" очистить  "),
				Span::styled("g", key_style),
				Span::raw(" генерация  "),
				Span::styled("/", key_style),
				Span::raw(" поиск  "),
				Span::styled("Shift+C/N/R/D/L", key_style),
				Span::raw(" вкладки  "),
				Span::styled("Esc", key_style),
				Span::raw(" выход"),
				marks_counter,
			])
		} else {
			Line::from(vec![
				Span::raw(" "),
				Span::styled("Shift+C/N/R/D/L", key_style),
				Span::raw(" вкладки  "),
				Span::styled("F5", key_style),
				Span::raw(" сборка  "),
				Span::styled("Shift+V", key_style),
				Span::raw(" видимость  "),
				Span::styled("Esc", key_style),
				Span::raw(" выход"),
			])
		};

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
				state.exit_network_search();
				Ok(true)
			}
			Action::SwitchToNetwork => {
				self.active_tab = ProjectTab::Network;
				state.active_buffer = crate::app::BufferKind::Network;
				Ok(true)
			}
			Action::SwitchToResources => {
				self.active_tab = ProjectTab::Resources;
				state.exit_network_search();
				Ok(true)
			}
			Action::SwitchToDom => {
				self.active_tab = ProjectTab::Dom;
				state.exit_network_search();
				Ok(true)
			}
			Action::SwitchToLogs => {
				self.active_tab = ProjectTab::Logs;
				state.active_buffer = crate::app::BufferKind::Logs;
				state.exit_network_search();
				Ok(true)
			}
			Action::ExitProject => {
				state.exit_network_search();
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
			Action::EnterNetworkSearch => {
				if self.active_tab == ProjectTab::Network {
					state.enter_network_search();
				}
				Ok(true)
			}
			Action::ExitNetworkSearch => {
				state.exit_network_search();
				Ok(true)
			}
			Action::NetworkSearchBackspace => {
				state.network_search_backspace();
				Ok(true)
			}
			Action::NetworkSearchInput(c) => {
				state.network_search_input_char(c);
				Ok(true)
			}
			Action::OpenNetworkInspect => {
				if self.active_tab == ProjectTab::Network {
					state.open_network_inspect();
				}
				Ok(true)
			}
			Action::NetworkMoveUp => {
				if self.active_tab == ProjectTab::Network {
					state.network_move_up();
				}
				Ok(true)
			}
			Action::NetworkMoveDown => {
				if self.active_tab == ProjectTab::Network {
					state.network_move_down();
				}
				Ok(true)
			}
			Action::NetworkPageUp => {
				if self.active_tab == ProjectTab::Network {
					state.network_page_up(10);
				}
				Ok(true)
			}
			Action::NetworkPageDown => {
				if self.active_tab == ProjectTab::Network {
					state.network_page_down(10);
				}
				Ok(true)
			}
			Action::NetworkHome => {
				if self.active_tab == ProjectTab::Network {
					state.network_home();
				}
				Ok(true)
			}
			Action::NetworkEnd => {
				if self.active_tab == ProjectTab::Network {
					state.network_end();
				}
				Ok(true)
			}
			Action::ToggleNetworkMark => {
				if self.active_tab == ProjectTab::Network {
					state.toggle_network_mark();
				}
				Ok(true)
			}
			Action::ClearNetworkMarks => {
				if self.active_tab == ProjectTab::Network {
					state.clear_network_marks();
				}
				Ok(true)
			}
			Action::GenerateApiFile => {
				if self.active_tab == ProjectTab::Network {
					let mut events = state.marked_events();
					let source = if events.is_empty() {
						let visible_indices = state.visible_network_indices();
						if visible_indices.is_empty() {
							log::warn!("Нет запросов для генерации. Список пуст.");
							return Ok(true);
						}
						let cursor = state.network_cursor.min(visible_indices.len().saturating_sub(1));
						let event_idx = visible_indices[cursor];
						if let Some(event) = state.network_events.get(event_idx) {
							events = vec![event];
							"current cursor"
						} else {
							log::warn!("Не удалось получить событие под курсором");
							return Ok(true);
						}
					} else {
						"marked events"
					};
					log::debug!("GenerateApiFile triggered. Source: {}, count: {}", source, events.len());

					let entry_path = match state.current_project() {
						Some(proj) => {
							let projects_base = match dirs::config_dir() {
								Some(base) => base.join("ewd-tui").join("projects"),
								None => {
									log::error!("Не удалось определить config_dir для сохранения API файла");
									return Ok(true);
								}
							};
							let path = projects_base.join(&proj.project.dir).join(&proj.project.config.entry);
							log::debug!("Resolved absolute entry path: {:?}", path);
							path
						}
						None => {
							log::error!("Нет активного проекта для определения пути сохранения.");
							return Ok(true);
						}
					};

					match crate::views::codegen::generate_api_file(&events, &entry_path) {
						Ok(output_path) => {
							log::info!(
								"✅ Сгенерирован файл API: {} ({} функций из {})",
								output_path.display(),
								events.len(),
								source
							);
						}
						Err(e) => {
							log::error!("❌ Ошибка генерации API файла: {}", e);
						}
					}
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
