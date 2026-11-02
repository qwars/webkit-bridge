use crate::app::AppState;

impl AppState {
	pub fn enter_search(&mut self) {
		self.search_mode = true;
		self.search_query.clear();
		self.selected_index = 0;
	}

	pub fn exit_search(&mut self) {
		let real = self.selected_real_index().unwrap_or(0);
		self.search_mode = false;
		self.search_query.clear();
		self.selected_index = real.min(self.projects.len().saturating_sub(1));
	}

	pub fn search_input_char(&mut self, c: char) {
		self.search_query.push(c);
		self.selected_index = 0;
	}

	pub fn search_backspace(&mut self) {
		self.search_query.pop();
		self.selected_index = 0;
	}

	pub fn search_submit(&mut self) {
		self.enter_project();
	}
}
