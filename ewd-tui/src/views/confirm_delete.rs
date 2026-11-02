use anyhow::Result;
use ratatui::{
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph},
	Frame,
};

use crate::app::AppState;
use crate::keys::Action;
use crate::views::View;

pub struct ConfirmDeleteView;

impl View for ConfirmDeleteView {
	fn draw(&self, f: &mut Frame, state: &AppState) {
		let area = f.area();

		let title = Block::default()
			.borders(Borders::ALL)
			.title(" Подтверждение удаления ")
			.title_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));

		let inner = title.inner(area);
		f.render_widget(title, area);

		if let Some(ref dir) = state.confirm_delete {
			let project = state.projects.iter().find(|p| p.project.dir == *dir);
			let name = project.map(|p| p.project.config.name.as_str()).unwrap_or(dir);

			let message = Paragraph::new(vec![
				Line::from(""),
				Line::from(vec![
					Span::raw("Вы действительно хотите удалить проект "),
					Span::styled(
						format!("'{}'?", name),
						Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
					),
				]),
				Line::from(""),
				Line::from(vec![
					Span::styled("y", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
					Span::raw(" — удалить  "),
					Span::styled("n", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
					Span::raw(" / "),
					Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
					Span::raw(" — отмена"),
				]),
			]);

			f.render_widget(message, inner);
		}
	}

	async fn handle_action(&mut self, action: Action, state: &mut AppState) -> Result<bool> {
		match action {
			Action::ConfirmDelete => {
				state.confirm_delete()?;
				Ok(true)
			}
			Action::CancelDelete => {
				state.cancel_delete();
				Ok(true)
			}
			_ => Ok(false),
		}
	}
}
