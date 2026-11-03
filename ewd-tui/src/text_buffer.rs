use ansi_to_tui::IntoText;
use ratatui::style::{Color as RatatuiColor, Style};
use ratatui::text::{Line, Span};
use ratatui_core::style::Color as CoreColor;

#[derive(Debug, Clone)]
pub struct BufferLine {
	pub text: String,
	pub line: Line<'static>,
}

#[derive(Debug, Clone)]
pub struct TextBuffer {
	pub lines: Vec<BufferLine>,
	pub cursor_row: usize,
	pub cursor_col: usize,
	pub anchor_row: usize,
	pub anchor_col: usize,
	pub scroll_offset: usize,
	pub auto_scroll: bool,
	pub max_lines: Option<usize>,
}

impl TextBuffer {
	pub fn new_with_limit(max_lines: usize) -> Self {
		Self {
			lines: Vec::new(),
			cursor_row: 0,
			cursor_col: 0,
			anchor_row: 0,
			anchor_col: 0,
			scroll_offset: 0,
			auto_scroll: true,
			max_lines: Some(max_lines),
		}
	}

	pub fn push_line(&mut self, line: String) {
		self.push_line_styled(line, Style::default());
	}

	pub fn push_line_styled(&mut self, text: String, style: Style) {
		let line = Line::from(vec![Span::styled(text.clone(), style)]);
		self.lines.push(BufferLine { text, line });
		self.trim_and_adjust_cursor();
	}

	pub fn push_ansi_line(&mut self, text: String) {
		let parsed = text.clone().into_text();
		match parsed {
			Ok(core_text) => {
				for core_line in core_text.lines {
					let mut stripped = String::new();
					let mut ratatui_spans = Vec::new();
					for core_span in core_line.spans {
						stripped.push_str(&core_span.content);
						let mut style = Style::default();
						if let Some(fg) = core_span.style.fg {
							style = style.fg(convert_color(fg));
						}
						if let Some(bg) = core_span.style.bg {
							style = style.bg(convert_color(bg));
						}
						ratatui_spans.push(Span::styled(core_span.content, style));
					}
					self.lines.push(BufferLine {
						text: stripped,
						line: Line::from(ratatui_spans),
					});
				}
			}
			Err(_) => {
				for line in text.split('\n') {
					self.lines.push(BufferLine {
						text: line.to_string(),
						line: Line::raw(line.to_string()),
					});
				}
			}
		}
		self.trim_and_adjust_cursor();
	}

	fn trim_and_adjust_cursor(&mut self) {
		if let Some(max) = self.max_lines {
			if self.lines.len() > max {
				let excess = self.lines.len() - max;
				self.lines.drain(0..excess);
				if self.scroll_offset >= excess {
					self.scroll_offset -= excess;
				} else {
					self.scroll_offset = 0;
				}
				if self.cursor_row >= excess {
					self.cursor_row -= excess;
				} else {
					self.cursor_row = 0;
					self.cursor_col = 0;
				}
				if self.anchor_row >= excess {
					self.anchor_row -= excess;
				} else {
					self.anchor_row = 0;
					self.anchor_col = 0;
				}
			}
		}
		if self.auto_scroll {
			self.move_to_buffer_end();
		}
	}

	pub fn clear(&mut self) {
		self.lines.clear();
		self.cursor_row = 0;
		self.cursor_col = 0;
		self.anchor_row = 0;
		self.anchor_col = 0;
		self.scroll_offset = 0;
		self.auto_scroll = true;
	}

	pub fn move_up(&mut self) {
		if self.cursor_row > 0 {
			self.cursor_row -= 1;
			self.anchor_row = self.cursor_row;
			self.anchor_col = self.cursor_col;
			self.auto_scroll = false;
		}
	}

	pub fn move_down(&mut self) {
		if self.cursor_row < self.lines.len().saturating_sub(1) {
			self.cursor_row += 1;
			self.anchor_row = self.cursor_row;
			self.anchor_col = self.cursor_col;
			if self.cursor_row == self.lines.len().saturating_sub(1) {
				self.auto_scroll = true;
			}
		}
	}

	pub fn move_left(&mut self) {
		if self.cursor_col > 0 {
			self.cursor_col -= 1;
			self.anchor_col = self.cursor_col;
		} else if self.cursor_row > 0 {
			self.cursor_row -= 1;
			self.cursor_col = self.lines[self.cursor_row].text.len();
			self.anchor_row = self.cursor_row;
			self.anchor_col = self.cursor_col;
		}
		self.auto_scroll = false;
	}

	pub fn move_right(&mut self) {
		let line_len = self.lines.get(self.cursor_row).map(|l| l.text.len()).unwrap_or(0);
		if self.cursor_col < line_len {
			self.cursor_col += 1;
			self.anchor_col = self.cursor_col;
		} else if self.cursor_row < self.lines.len().saturating_sub(1) {
			self.cursor_row += 1;
			self.cursor_col = 0;
			self.anchor_row = self.cursor_row;
			self.anchor_col = self.cursor_col;
			if self.cursor_row == self.lines.len().saturating_sub(1) {
				self.auto_scroll = true;
			}
		}
	}

	pub fn page_up(&mut self, page_size: usize) {
		self.cursor_row = self.cursor_row.saturating_sub(page_size);
		self.anchor_row = self.cursor_row;
		self.anchor_col = self.cursor_col;
		self.auto_scroll = false;
	}

	pub fn page_down(&mut self, page_size: usize) {
		self.cursor_row = (self.cursor_row + page_size).min(self.lines.len().saturating_sub(1));
		self.anchor_row = self.cursor_row;
		self.anchor_col = self.cursor_col;
		if self.cursor_row == self.lines.len().saturating_sub(1) {
			self.auto_scroll = true;
		}
	}

	pub fn move_to_buffer_start(&mut self) {
		self.cursor_row = 0;
		self.cursor_col = 0;
		self.anchor_row = 0;
		self.anchor_col = 0;
		self.auto_scroll = false;
	}

	pub fn move_to_buffer_end(&mut self) {
		if self.lines.is_empty() {
			self.cursor_row = 0;
			self.cursor_col = 0;
		} else {
			self.cursor_row = self.lines.len() - 1;
			self.cursor_col = self.lines[self.cursor_row].text.len();
		}
		self.anchor_row = self.cursor_row;
		self.anchor_col = self.cursor_col;
		self.auto_scroll = true;
	}

	pub fn select_up(&mut self) {
		if self.cursor_row > 0 {
			self.cursor_row -= 1;
		}
	}

	pub fn select_down(&mut self) {
		if self.cursor_row < self.lines.len().saturating_sub(1) {
			self.cursor_row += 1;
		}
	}

	pub fn select_left(&mut self) {
		if self.cursor_col > 0 {
			self.cursor_col -= 1;
		} else if self.cursor_row > 0 {
			self.cursor_row -= 1;
			self.cursor_col = self.lines[self.cursor_row].text.len();
		}
	}

	pub fn select_right(&mut self) {
		let line_len = self.lines.get(self.cursor_row).map(|l| l.text.len()).unwrap_or(0);
		if self.cursor_col < line_len {
			self.cursor_col += 1;
		} else if self.cursor_row < self.lines.len().saturating_sub(1) {
			self.cursor_row += 1;
			self.cursor_col = 0;
		}
	}

	pub fn select_all(&mut self) {
		self.anchor_row = 0;
		self.anchor_col = 0;
		if self.lines.is_empty() {
			self.cursor_row = 0;
			self.cursor_col = 0;
		} else {
			self.cursor_row = self.lines.len() - 1;
			self.cursor_col = self.lines[self.cursor_row].text.len();
		}
		self.auto_scroll = true;
	}

	pub fn toggle_mark(&mut self) {
		self.anchor_row = self.cursor_row;
		self.anchor_col = self.cursor_col;
	}

	pub fn has_selection(&self) -> bool {
		self.cursor_row != self.anchor_row || self.cursor_col != self.anchor_col
	}

	pub fn selection_range(&self) -> Option<(usize, usize, usize, usize)> {
		if !self.has_selection() {
			return None;
		}
		let (start_row, start_col, end_row, end_col) = if self.cursor_row < self.anchor_row
			|| (self.cursor_row == self.anchor_row && self.cursor_col < self.anchor_col)
		{
			(self.cursor_row, self.cursor_col, self.anchor_row, self.anchor_col)
		} else {
			(self.anchor_row, self.anchor_col, self.cursor_row, self.cursor_col)
		};
		Some((start_row, start_col, end_row, end_col))
	}

	pub fn selected_text(&self) -> Option<String> {
		if !self.has_selection() {
			return None;
		}
		let (start_row, start_col, end_row, end_col) = self.selection_range()?;
		let mut result = String::new();
		for row in start_row..=end_row {
			if row >= self.lines.len() {
				break;
			}
			let line_text = &self.lines[row].text;
			let line_len = line_text.len();
			let start = if row == start_row { start_col } else { 0 };
			let end = if row == end_row {
				end_col.min(line_len)
			} else {
				line_len
			};
			if start < line_len {
				result.push_str(&line_text[start..end]);
			}
			if row < end_row {
				result.push('\n');
			}
		}
		Some(result)
	}

	pub fn update_scroll(&mut self, viewport_height: usize) {
		if viewport_height == 0 {
			return;
		}
		if self.auto_scroll {
			self.scroll_offset = self.lines.len().saturating_sub(viewport_height);
		} else {
			if self.cursor_row < self.scroll_offset {
				self.scroll_offset = self.cursor_row;
			} else if self.cursor_row >= self.scroll_offset + viewport_height {
				self.scroll_offset = self.cursor_row - viewport_height + 1;
			}
		}
	}

	pub fn visible_lines(&self, viewport_height: usize) -> (usize, usize) {
		let start = self.scroll_offset;
		let end = (start + viewport_height).min(self.lines.len());
		(start, end)
	}
}

fn convert_color(c: CoreColor) -> RatatuiColor {
	match c {
		CoreColor::Reset => RatatuiColor::Reset,
		CoreColor::Black => RatatuiColor::Black,
		CoreColor::Red => RatatuiColor::Red,
		CoreColor::Green => RatatuiColor::Green,
		CoreColor::Yellow => RatatuiColor::Yellow,
		CoreColor::Blue => RatatuiColor::Blue,
		CoreColor::Magenta => RatatuiColor::Magenta,
		CoreColor::Cyan => RatatuiColor::Cyan,
		CoreColor::Gray => RatatuiColor::Gray,
		CoreColor::DarkGray => RatatuiColor::DarkGray,
		CoreColor::LightRed => RatatuiColor::LightRed,
		CoreColor::LightGreen => RatatuiColor::LightGreen,
		CoreColor::LightYellow => RatatuiColor::LightYellow,
		CoreColor::LightBlue => RatatuiColor::LightBlue,
		CoreColor::LightMagenta => RatatuiColor::LightMagenta,
		CoreColor::LightCyan => RatatuiColor::LightCyan,
		CoreColor::White => RatatuiColor::White,
		CoreColor::Rgb(r, g, b) => RatatuiColor::Rgb(r, g, b),
		CoreColor::Indexed(i) => RatatuiColor::Indexed(i),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_push_line_and_navigation() {
		let mut buffer = TextBuffer::new_with_limit(100);
		buffer.push_line("line 1".to_string());
		buffer.push_line("line 2".to_string());
		assert_eq!(buffer.lines.len(), 2);
		assert_eq!(buffer.cursor_row, 1);
		buffer.move_up();
		assert_eq!(buffer.cursor_row, 0);
	}

	#[test]
	fn test_max_lines_limit() {
		let mut buffer = TextBuffer::new_with_limit(5);
		for i in 0..10 {
			buffer.push_line(format!("line {}", i));
		}
		assert_eq!(buffer.lines.len(), 5);
		assert_eq!(buffer.lines[0].text, "line 5");
	}

	#[test]
	fn test_selected_text_single_line() {
		let mut buffer = TextBuffer::new_with_limit(100);
		buffer.push_line("hello world".to_string());
		buffer.cursor_col = 0;
		buffer.anchor_col = 0;
		for _ in 0..5 {
			buffer.select_right();
		}
		assert_eq!(buffer.selected_text(), Some("hello".to_string()));
	}

	#[test]
	fn test_auto_scroll_on_push() {
		let mut buffer = TextBuffer::new_with_limit(100);
		assert!(buffer.auto_scroll);
		buffer.push_line("line 1".to_string());
		assert_eq!(buffer.cursor_row, 0);
		buffer.push_line("line 2".to_string());
		assert_eq!(buffer.cursor_row, 1);
	}

	#[test]
	fn test_auto_scroll_disabled_on_move_up() {
		let mut buffer = TextBuffer::new_with_limit(100);
		buffer.push_line("line 1".to_string());
		buffer.push_line("line 2".to_string());
		buffer.push_line("line 3".to_string());
		assert!(buffer.auto_scroll);

		buffer.move_up();
		assert!(!buffer.auto_scroll);
		assert_eq!(buffer.cursor_row, 1);

		buffer.push_line("line 4".to_string());
		assert_eq!(buffer.cursor_row, 1);
		assert_eq!(buffer.lines.len(), 4);
	}

	#[test]
	fn test_auto_scroll_enabled_on_reach_bottom() {
		let mut buffer = TextBuffer::new_with_limit(100);
		buffer.push_line("line 1".to_string());
		buffer.push_line("line 2".to_string());
		buffer.push_line("line 3".to_string());
		buffer.move_up();
		assert!(!buffer.auto_scroll);
		buffer.move_down();
		buffer.move_down();
		assert!(buffer.auto_scroll);
	}
}
