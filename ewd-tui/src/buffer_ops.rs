use crate::app::{AppState, BufferKind};

impl AppState {
	pub fn get_active_buffer_mut(&mut self) -> &mut crate::text_buffer::TextBuffer {
		match self.active_buffer {
			BufferKind::Console => &mut self.console_buffer,
			BufferKind::Logs => &mut self.log_buffer,
		}
	}

	pub fn buffer_move_up(&mut self) {
		self.get_active_buffer_mut().move_up();
	}
	pub fn buffer_move_down(&mut self) {
		self.get_active_buffer_mut().move_down();
	}
	pub fn buffer_move_left(&mut self) {
		self.get_active_buffer_mut().move_left();
	}
	pub fn buffer_move_right(&mut self) {
		self.get_active_buffer_mut().move_right();
	}
	pub fn buffer_page_up(&mut self, page_size: usize) {
		self.get_active_buffer_mut().page_up(page_size);
	}
	pub fn buffer_page_down(&mut self, page_size: usize) {
		self.get_active_buffer_mut().page_down(page_size);
	}
	pub fn buffer_home(&mut self) {
		self.get_active_buffer_mut().move_to_buffer_start();
	}
	pub fn buffer_end(&mut self) {
		self.get_active_buffer_mut().move_to_buffer_end();
	}
	pub fn buffer_select_up(&mut self) {
		self.get_active_buffer_mut().select_up();
	}
	pub fn buffer_select_down(&mut self) {
		self.get_active_buffer_mut().select_down();
	}
	pub fn buffer_select_left(&mut self) {
		self.get_active_buffer_mut().select_left();
	}
	pub fn buffer_select_right(&mut self) {
		self.get_active_buffer_mut().select_right();
	}
	pub fn buffer_select_all(&mut self) {
		self.get_active_buffer_mut().select_all();
	}
	pub fn buffer_toggle_mark(&mut self) {
		self.get_active_buffer_mut().toggle_mark();
	}
	pub fn buffer_copy(&mut self) {
		if let Some(text) = self.get_active_buffer_mut().selected_text() {
			if let Some(clipboard) = &mut self.clipboard {
				let _ = clipboard.set_text(text);
			}
		}
	}
}
