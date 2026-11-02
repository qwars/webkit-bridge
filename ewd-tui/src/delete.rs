use anyhow::Result;

use crate::app::AppState;

impl AppState {
	pub fn request_delete(&mut self) {
		if let Some(real) = self.selected_real_index() {
			if let Some(project) = self.projects.get(real) {
				self.confirm_delete = Some(project.project.dir.clone());
				self.screen = crate::app::Screen::ConfirmDelete;
			}
		}
	}

	pub fn confirm_delete(&mut self) -> Result<()> {
		if let Some(dir) = self.confirm_delete.take() {
			self.stop_watcher(&dir);
			crate::projects::delete_project(&dir)?;
			self.projects.retain(|p| p.project.dir != dir);
			let visible = self.filtered_indices().len();
			if visible == 0 {
				self.selected_index = 0;
			} else if self.selected_index >= visible {
				self.selected_index = visible - 1;
			}
		}
		self.screen = crate::app::Screen::Projects;
		Ok(())
	}

	pub fn cancel_delete(&mut self) {
		self.confirm_delete = None;
		self.screen = crate::app::Screen::Projects;
	}
}
