pub mod codegen;
pub mod confirm_delete;
pub mod create_project;
pub mod network_inspect;
pub mod project;
pub mod projects;

use ratatui::Frame;

use crate::app::AppState;

pub trait View {
	fn draw(&self, f: &mut Frame, state: &AppState);
	async fn handle_action(&mut self, _action: crate::keys::Action, _state: &mut AppState) -> anyhow::Result<bool> {
		Ok(false)
	}
}
