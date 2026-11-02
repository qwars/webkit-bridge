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
