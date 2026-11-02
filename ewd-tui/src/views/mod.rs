use anyhow::Result;
use ratatui::Frame;

use crate::app::AppState;
use crate::keys::Action;

pub trait View {
	fn draw(&self, f: &mut Frame, state: &AppState);
	async fn handle_action(&mut self, action: Action, state: &mut AppState) -> Result<bool>;
}

pub mod confirm_delete;
pub mod create_project;
pub mod project;
pub mod projects;
