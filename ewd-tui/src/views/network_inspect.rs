use anyhow::Result;
use ewd_core::events::{NetworkEvent, NetworkEventType};
use ratatui::{
	layout::{Alignment, Rect},
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Padding, Paragraph},
	Frame,
};

use crate::app::AppState;
use crate::keys::Action;
use crate::text_buffer::TextBuffer;
use crate::views::View;

pub struct NetworkInspectView {
	pub buffer: TextBuffer,
	pub event_idx: usize,
}

impl NetworkInspectView {
	pub fn new(event_idx: usize, event: &NetworkEvent) -> Self {
		let mut buffer = TextBuffer::new_with_limit(1000);

		buffer.push_line_styled(
			"═══ Network Request Details ═══".to_string(),
			Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
		);
		buffer.push_line(String::new());

		buffer.push_line_styled(format!("URL:    {}", event.url), Style::default().fg(Color::White));
		buffer.push_line_styled(format!("Method: {}", event.method), Style::default().fg(Color::Yellow));

		let status_str = event
			.status_code
			.map(|s| s.to_string())
			.unwrap_or_else(|| "-".to_string());
		let status_color = match event.status_code {
			Some(200..=299) => Color::Green,
			Some(300..=399) => Color::Cyan,
			Some(400..=499) => Color::Yellow,
			Some(_) => Color::Red,
			None => Color::DarkGray,
		};
		buffer.push_line_styled(format!("Status: {}", status_str), Style::default().fg(status_color));

		let time_str = event
			.response_time_ms
			.map(|t| format!("{}ms", t))
			.unwrap_or_else(|| "-".to_string());
		buffer.push_line_styled(format!("Time:   {}", time_str), Style::default().fg(Color::White));

		let type_str = match event.event_type {
			NetworkEventType::Started => "started",
			NetworkEventType::Finished => "finished",
			NetworkEventType::Failed => "failed",
		};
		buffer.push_line_styled(format!("Type:   {}", type_str), Style::default().fg(Color::White));

		buffer.push_line(String::new());

		buffer.push_line_styled(
			"─── Request Headers ───".to_string(),
			Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
		);
		if event.request_headers.is_empty() {
			buffer.push_line_styled("  (none)".to_string(), Style::default().fg(Color::DarkGray));
		} else {
			for (name, value) in &event.request_headers {
				buffer.push_line(format!("  {}: {}", name, value));
			}
		}

		buffer.push_line(String::new());

		buffer.push_line_styled(
			"─── Response Headers ───".to_string(),
			Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
		);
		if event.response_headers.is_empty() {
			buffer.push_line_styled("  (none)".to_string(), Style::default().fg(Color::DarkGray));
		} else {
			for (name, value) in &event.response_headers {
				buffer.push_line(format!("  {}: {}", name, value));
			}
		}

		Self { buffer, event_idx }
	}
}

impl View for NetworkInspectView {
	fn draw(&self, f: &mut Frame, state: &AppState) {
		log::debug!("NetworkInspectView::draw called, event_idx: {}", self.event_idx);
		log::debug!("buffer.lines.len(): {}", self.buffer.lines.len());

		let area = f.area();
		let event = match state.network_events.get(self.event_idx) {
			Some(e) => {
				log::debug!("Event found in draw: {} {}", e.method, e.url);
				e
			}
			None => {
				log::error!("Event not found in draw for event_idx: {}", self.event_idx);
				return;
			}
		};
		let title = Line::from(vec![Span::styled(
			format!(" Inspect: {} {} ", event.method, truncate_url(&event.url, 60)),
			Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
		)]);

		let block = Block::default()
			.borders(Borders::ALL)
			.title_top(title.alignment(Alignment::Left))
			.padding(Padding::uniform(1));

		let inner = block.inner(area);
		f.render_widget(block, area);

		if self.buffer.lines.is_empty() {
			return;
		}

		let area_height = inner.height as usize;
		let mut buffer_clone = self.buffer.clone();
		buffer_clone.update_scroll(area_height);
		let (start, end) = buffer_clone.visible_lines(area_height);

		let visible_lines: Vec<Line> = buffer_clone.lines[start..end]
			.iter()
			.map(|buffer_line| buffer_line.line.clone())
			.collect();

		f.render_widget(Paragraph::new(visible_lines), inner);

		let hints_area = Rect {
			x: area.x,
			y: area.height.saturating_sub(1),
			width: area.width,
			height: 1,
		};
		let key_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
		let hints = Line::from(vec![
			Span::raw(" "),
			Span::styled("↑/↓", key_style),
			Span::raw(" прокрутка  "),
			Span::styled("Esc", key_style),
			Span::raw(" назад"),
		]);
		f.render_widget(Paragraph::new(hints).alignment(Alignment::Left), hints_area);
	}

	async fn handle_action(&mut self, action: Action, _state: &mut AppState) -> Result<bool> {
		match action {
			Action::CloseNetworkInspect => Ok(true),
			Action::BufferMoveUp => {
				self.buffer.move_up();
				Ok(true)
			}
			Action::BufferMoveDown => {
				self.buffer.move_down();
				Ok(true)
			}
			Action::BufferPageUp => {
				self.buffer.page_up(10);
				Ok(true)
			}
			Action::BufferPageDown => {
				self.buffer.page_down(10);
				Ok(true)
			}
			Action::BufferHome => {
				self.buffer.move_to_buffer_start();
				Ok(true)
			}
			Action::BufferEnd => {
				self.buffer.move_to_buffer_end();
				Ok(true)
			}
			_ => Ok(false),
		}
	}
}

fn truncate_url(url: &str, max_len: usize) -> String {
	if url.len() <= max_len {
		url.to_string()
	} else {
		format!("{}...", &url[..max_len.saturating_sub(3)])
	}
}
