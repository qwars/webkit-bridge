use crate::app::AppState;

impl AppState {
	pub fn filtered_indices(&self) -> Vec<usize> {
		if !self.search_mode || self.search_query.is_empty() {
			return (0..self.projects.len()).collect();
		}
		let query = self.search_query.to_lowercase();
		self.projects
			.iter()
			.enumerate()
			.filter(|(_, p)| p.project.config.name.to_lowercase().contains(&query))
			.map(|(i, _)| i)
			.collect()
	}

	pub fn filtered_projects(&self) -> Vec<&crate::app::ProjectState> {
		self.filtered_indices()
			.into_iter()
			.filter_map(|i| self.projects.get(i))
			.collect()
	}

	pub(crate) fn selected_real_index(&self) -> Option<usize> {
		self.filtered_indices().get(self.selected_index).copied()
	}

	pub fn navigate_up(&mut self) {
		if self.selected_index > 0 {
			self.selected_index -= 1;
		}
	}

	pub fn navigate_down(&mut self) {
		let count = self.filtered_indices().len();
		if self.selected_index < count.saturating_sub(1) {
			self.selected_index += 1;
		}
	}

	pub fn enter_project(&mut self) {
		if let Some(real) = self.selected_real_index() {
			if let Some(project) = self.projects.get(real) {
				let dir = project.project.dir.clone();
				self.selected_index = real;
				self.search_mode = false;
				self.search_query.clear();
				self.console_buffer.clear();
				self.screen = crate::app::Screen::Project(dir.clone());

				if let Err(e) = self.ensure_project_running(&dir) {
					log::error!("Failed to ensure project running: {}", e);
				}
			}
		}
	}
}
