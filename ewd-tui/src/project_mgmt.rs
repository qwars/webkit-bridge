use anyhow::Result;
use ratatui::style::{Color, Style};
use std::time::SystemTime;

use crate::app::{AppState, BuildResult, ProjectStatus};
use crate::build::{build_esbuild_command, collect_build_output, spawn_esbuild};
use crate::watcher::ProjectWatcher;

impl AppState {
	pub fn ensure_project_running(&mut self, dir: &str) -> Result<()> {
		if let Some(project) = self.projects.iter().find(|p| p.project.dir == dir) {
			if project.status == ProjectStatus::Running {
				return Ok(());
			}
		}

		self.pending_start = Some(dir.to_string());

		let bundle_mtime = crate::projects::get_bundle_mtime(dir);
		let sources_mtime = crate::projects::get_sources_mtime(dir);

		let needs_build = match (bundle_mtime, sources_mtime) {
			(Err(_), _) => {
				log::info!("Bundle not found for '{}', building", dir);
				true
			}
			(Ok(bundle), Ok(sources)) => {
				if sources > bundle {
					log::info!("Sources newer than bundle for '{}', building", dir);
					true
				} else {
					log::info!("Bundle up-to-date for '{}', skipping build", dir);
					false
				}
			}
			(Ok(_), Err(e)) => {
				log::warn!("Failed to get sources mtime for '{}': {}", dir, e);
				true
			}
		};

		if needs_build {
			self.start_build(dir)?;
		}

		Ok(())
	}

	pub fn start_build(&mut self, dir: &str) -> Result<()> {
		let project = match self.projects.iter().find(|p| p.project.dir == dir) {
			Some(p) => p.project.clone(),
			None => return Ok(()),
		};

		if let Some(handle) = self.active_builds.remove(dir) {
			handle.abort();
			log::info!("Aborted old build for '{}'", dir);
		}

		self.next_build_id += 1;
		let build_id = self.next_build_id;
		self.active_build_id.insert(dir.to_string(), build_id);

		self.console_buffer.clear();
		self.is_building = true;

		let (cmd, args) = build_esbuild_command(&project)?;
		self.console_buffer.push_line_styled(
			format!("$ {} {}", cmd, args.join(" ")),
			Style::default().fg(Color::DarkGray),
		);

		let tx = self.build_tx.clone();
		let dir_owned = dir.to_string();
		let bundle_path = crate::projects::bundle_path(dir)?;

		let handle = tokio::spawn(async move {
			let output = async {
				let mut child = spawn_esbuild(&project).await?;
				collect_build_output(&mut child, bundle_path).await
			}
			.await;

			let _ = tx.send(BuildResult {
				dir: dir_owned,
				output,
				build_id,
			});
		});

		self.active_builds.insert(dir.to_string(), handle);

		Ok(())
	}

	pub fn drain_build_results(&mut self) {
		let rx = match self.build_rx.as_mut() {
			Some(rx) => rx,
			None => return,
		};

		loop {
			match rx.try_recv() {
				Ok(result) => {
					let is_current = self.active_build_id.get(&result.dir) == Some(&result.build_id);

					if !is_current {
						log::debug!("Ignoring stale build result for '{}'", result.dir);
						continue;
					}

					match result.output {
						Ok(output) => {
							for line in output.stdout.lines() {
								self.console_buffer.push_ansi_line(line.to_string());
							}
							for line in output.stderr.lines() {
								self.console_buffer.push_ansi_line(line.to_string());
							}

							if output.success {
								let msg = format!("Сборка успешна: {}", output.bundle_path.display());
								self.console_buffer
									.push_line_styled(msg, Style::default().fg(Color::Green));

								if let Some(project) = self.projects.iter_mut().find(|p| p.project.dir == result.dir) {
									project.mtime = SystemTime::now();
									project.last_build_success = Some(true);
								}

								if self.pending_start.as_deref() == Some(&result.dir) {
									self.pending_start = None;
								} else {
									let should_restart = self
										.projects
										.iter()
										.find(|p| p.project.dir == result.dir)
										.map(|p| p.status == ProjectStatus::Running)
										.unwrap_or(false);
									if should_restart {
										let dir = result.dir.clone();
										self.pending_start = Some(dir);
									}
								}
							} else {
								self.console_buffer
									.push_line_styled("Сборка не удалась".to_string(), Style::default().fg(Color::Red));
								if let Some(project) = self.projects.iter_mut().find(|p| p.project.dir == result.dir) {
									project.last_build_success = Some(false);
								}
								if self.pending_start.as_deref() == Some(&result.dir) {
									self.pending_start = None;
								}
							}
						}
						Err(e) => {
							self.console_buffer
								.push_line_styled(format!("Ошибка сборки: {}", e), Style::default().fg(Color::Red));
							if let Some(project) = self.projects.iter_mut().find(|p| p.project.dir == result.dir) {
								project.last_build_success = Some(false);
							}
							if self.pending_start.as_deref() == Some(&result.dir) {
								self.pending_start = None;
							}
						}
					}

					self.active_build_id.remove(&result.dir);
					self.active_builds.remove(&result.dir);

					self.is_building = !self.active_build_id.is_empty();
				}
				Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
				Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
			}
		}
	}

	pub async fn start_project(&mut self, dir: &str) -> Result<()> {
		if let Some(ipc) = &self.ipc_client {
			if let Some(project) = self.projects.iter_mut().find(|p| p.project.dir == dir) {
				if project.status == ProjectStatus::Stopped {
					let tab_id = ipc.create(&project.project.config.url, project.tab_visible).await?;
					project.tab_id = Some(tab_id);
					project.status = ProjectStatus::Running;
					self.start_watcher(dir);
				}
			}
		}
		Ok(())
	}

	pub async fn stop_project(&mut self, dir: &str) -> Result<()> {
		if let Some(ipc) = &self.ipc_client {
			if let Some(project) = self.projects.iter_mut().find(|p| p.project.dir == dir) {
				if project.status == ProjectStatus::Running {
					if let Some(ref tab_id) = project.tab_id {
						let _ = ipc.close(tab_id).await;
					}
					project.tab_id = None;
					project.status = ProjectStatus::Stopped;
					self.stop_watcher(dir);
				}
			}
		}
		Ok(())
	}

	pub async fn toggle_tab_visibility(&mut self, dir: &str) -> Result<()> {
		if let Some(ipc) = &self.ipc_client {
			if let Some(project) = self.projects.iter_mut().find(|p| p.project.dir == dir) {
				if project.status == ProjectStatus::Running {
					if project.tab_visible {
						if let Some(ref tab_id) = project.tab_id {
							ipc.hide(tab_id).await?;
						}
						project.tab_visible = false;
					} else {
						if let Some(ref tab_id) = project.tab_id {
							ipc.show(tab_id).await?;
						}
						project.tab_visible = true;
					}
				}
			}
		}
		Ok(())
	}

	pub async fn stop_and_exit_project(&mut self) -> Result<()> {
		if let crate::app::Screen::Project(dir) = &self.screen {
			let dir = dir.clone();
			self.pending_start = None;
			self.stop_project(&dir).await?;
		}
		self.screen = crate::app::Screen::Projects;
		Ok(())
	}

	pub fn start_watcher(&mut self, dir: &str) {
		if self.active_watchers.contains_key(dir) {
			return;
		}
		if let Some(project) = self.projects.iter().find(|p| p.project.dir == dir) {
			let bundle_path = match crate::projects::bundle_path(dir) {
				Ok(path) => path,
				Err(e) => {
					log::error!("Failed to get bundle path for '{}': {}", dir, e);
					return;
				}
			};

			let watcher = ProjectWatcher::start(project.project.clone(), self.watcher_tx.clone(), bundle_path);
			self.active_watchers.insert(dir.to_string(), watcher);
			log::info!("Watcher started for '{}'", dir);
		}
	}

	pub fn stop_watcher(&mut self, dir: &str) {
		if let Some(watcher) = self.active_watchers.remove(dir) {
			watcher.stop();
			log::info!("Watcher stopped for '{}'", dir);
		}
	}

	pub async fn drain_watcher_events(&mut self) {
		loop {
			match self.watcher_rx.as_mut().map(|rx| rx.try_recv()) {
				Some(Ok(event)) => {
					log::info!("Build event for '{}'", event.dir);
					if let Err(e) = self.start_build(&event.dir) {
						log::error!("Failed to start build: {}", e);
					}
				}
				Some(Err(_)) => break,
				None => break,
			}
		}
	}
}
