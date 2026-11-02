use anyhow::Result;
use ratatui::{
	layout::{Constraint, Layout, Margin, Rect},
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph},
	Frame,
};

use crate::app::AppState;
use crate::create_project::CreateProjectField;
use crate::keys::Action;
use crate::views::View;

pub struct CreateProjectView;

impl View for CreateProjectView {
	fn draw(&self, f: &mut Frame, state: &AppState) {
		let area = f.area();
		let form = &state.create_project_state;

		let block = Block::default()
			.borders(Borders::ALL)
			.title(" Создание проекта ")
			.title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
		f.render_widget(block, area);

		let inner = area.inner(Margin {
			vertical: 1,
			horizontal: 1,
		});

		let chunks = Layout::default()
			.direction(ratatui::layout::Direction::Vertical)
			.constraints([
				Constraint::Length(3),
				Constraint::Length(3),
				Constraint::Length(3),
				Constraint::Length(3),
				Constraint::Max(4),
			])
			.split(inner);

		let fields = [
			("Название проекта", &form.name, CreateProjectField::Name),
			("Каталог (имя папки)", &form.dir, CreateProjectField::Dir),
			("URL", &form.url, CreateProjectField::Url),
			("Entry point", &form.entry, CreateProjectField::Entry),
		];

		for (i, (label, field, field_type)) in fields.iter().enumerate() {
			let is_focused = form.focused_field == *field_type;
			let is_invalid = form.invalid_fields.contains(field_type);

			let border_style = if is_invalid {
				Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
			} else if is_focused {
				Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(Color::White)
			};

			let field_block = Block::default()
				.borders(Borders::ALL)
				.title(format!(" {} ", label))
				.border_style(border_style);

			let mut spans = Vec::new();
			if is_focused {
				let chars: Vec<char> = field.text.chars().collect();
				let selection = field.selection_range();

				for (idx, &c) in chars.iter().enumerate() {
					let is_selected = selection.map_or(false, |(start, end)| idx >= start && idx < end);
					let is_cursor = idx == field.cursor;

					let style = if is_selected {
						Style::default().add_modifier(Modifier::REVERSED)
					} else if is_cursor && !field.has_selection() {
						Style::default().add_modifier(Modifier::REVERSED)
					} else {
						Style::default()
					};

					spans.push(Span::styled(c.to_string(), style));
				}

				if field.cursor >= chars.len() && !field.has_selection() {
					spans.push(Span::styled(" ", Style::default().add_modifier(Modifier::REVERSED)));
				}
			} else {
				spans.push(Span::raw(field.text.clone()));
			}

			let paragraph = Paragraph::new(Line::from(spans)).style(Style::default().fg(Color::White));
			f.render_widget(paragraph.block(field_block), chunks[i]);
		}

		if let Some(ref err) = form.error {
			let error_block = Block::default()
				.borders(Borders::ALL)
				.border_style(Style::default().fg(Color::Red));
			let error_para = Paragraph::new(err.as_str()).style(Style::default().fg(Color::Red));
			f.render_widget(error_para.block(error_block), chunks[4]);
		} else {
			let hint_para = Paragraph::new("Каталог используется как имя папки в ~/.config/ewd-tui/projects/")
				.style(Style::default().fg(Color::DarkGray));
			f.render_widget(hint_para, chunks[4]);
		}

		let hints_area = Rect {
			x: area.x,
			y: area.height.saturating_sub(1),
			width: area.width,
			height: 1,
		};

		let hints = Paragraph::new(Line::from(vec![
			Span::raw(" "),
			Span::styled("Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
			Span::raw(" след. поле  "),
			Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
			Span::raw(" создать  "),
			Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
			Span::raw(" отмена"),
		]));
		f.render_widget(hints, hints_area);
	}

	async fn handle_action(&mut self, action: Action, state: &mut AppState) -> Result<bool> {
		match action {
			Action::InputChar(c) => {
				state.input_char(c);
				Ok(true)
			}
			Action::Backspace => {
				state.backspace();
				Ok(true)
			}
			Action::Delete => {
				state.delete();
				Ok(true)
			}
			Action::MoveLeft => {
				state.move_cursor_left();
				Ok(true)
			}
			Action::MoveRight => {
				state.move_cursor_right();
				Ok(true)
			}
			Action::MoveToLineStart => {
				state.move_to_line_start();
				Ok(true)
			}
			Action::MoveToLineEnd => {
				state.move_to_line_end();
				Ok(true)
			}
			Action::MoveWordLeft => {
				state.move_word_left();
				Ok(true)
			}
			Action::MoveWordRight => {
				state.move_word_right();
				Ok(true)
			}
			Action::SelectLeft => {
				state.select_left();
				Ok(true)
			}
			Action::SelectRight => {
				state.select_right();
				Ok(true)
			}
			Action::SelectToLineStart => {
				state.select_to_line_start();
				Ok(true)
			}
			Action::SelectToLineEnd => {
				state.select_to_line_end();
				Ok(true)
			}
			Action::SelectWordLeft => {
				state.select_word_left();
				Ok(true)
			}
			Action::SelectWordRight => {
				state.select_word_right();
				Ok(true)
			}
			Action::SelectAll => {
				state.select_all();
				Ok(true)
			}
			Action::Copy => {
				state.copy();
				Ok(true)
			}
			Action::Cut => {
				state.cut();
				Ok(true)
			}
			Action::Paste => {
				state.paste();
				Ok(true)
			}
			Action::CutWord => {
				state.cut_word();
				Ok(true)
			}
			Action::KillLine => {
				state.kill_line();
				Ok(true)
			}
			Action::BackwardKillLine => {
				state.backward_kill_line();
				Ok(true)
			}
			Action::NextField => {
				state.next_field();
				Ok(true)
			}
			Action::SubmitForm => {
				state.submit_create_project().await?;
				Ok(true)
			}
			Action::CancelForm => {
				state.cancel_create_project();
				Ok(true)
			}
			_ => Ok(false),
		}
	}
}
