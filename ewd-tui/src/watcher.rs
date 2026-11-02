use notify::{Event, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::projects::Project;

const DEBOUNCE_DELAY: Duration = Duration::from_millis(200);

/// Событие сборки, отправляемое watcher'ом
#[derive(Debug, Clone)]
pub struct BuildEvent {
	pub dir: String,
}

/// Watcher для отслеживания изменений файлов проекта
pub struct ProjectWatcher {
	_handle: JoinHandle<()>,
	stop_tx: mpsc::UnboundedSender<()>,
}

impl ProjectWatcher {
	/// Запустить watcher для проекта
	pub fn start(project: Project, event_tx: mpsc::UnboundedSender<BuildEvent>) -> Self {
		let (stop_tx, mut stop_rx) = mpsc::unbounded_channel::<()>();
		let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<Event>();

		let dir = project.dir.clone();

		let watch_dir = match crate::projects::project_dir(&dir) {
			Ok(path) => path,
			Err(e) => {
				log::error!("Failed to get project dir for watcher '{}': {}", dir, e);
				return Self {
					_handle: tokio::spawn(async {}),
					stop_tx,
				};
			}
		};

		let extensions: HashSet<String> = project
			.config
			.watch_extensions
			.iter()
			.map(|ext| ext.trim_start_matches('.').to_ascii_lowercase())
			.collect();

		let handle = tokio::spawn(async move {
			let callback_extensions = extensions.clone();

			let mut watcher = match notify::recommended_watcher(move |res: Result<Event, notify::Error>| match res {
				Ok(event) => {
					let should_build = event
						.paths
						.iter()
						.any(|path| should_watch_path(path, &callback_extensions));

					if should_build {
						let _ = raw_tx.send(event);
					}
				}
				Err(e) => {
					log::warn!("Watcher error: {}", e);
				}
			}) {
				Ok(watcher) => watcher,
				Err(e) => {
					log::error!("Failed to create watcher for '{}': {}", dir, e);
					return;
				}
			};

			if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::Recursive) {
				log::error!("Failed to watch {:?} for '{}': {}", watch_dir, dir, e);
				return;
			}

			log::info!("Watcher started for project '{}'", dir);

			'watcher_loop: loop {
				tokio::select! {
					_ = stop_rx.recv() => {
						break 'watcher_loop;
					}

					event = raw_rx.recv() => {
						if event.is_none() {
							break 'watcher_loop;
						}

						// Debounce: ждём 200ms тишины после последнего события.
						loop {
							tokio::select! {
								_ = stop_rx.recv() => {
									break 'watcher_loop;
								}

								next_event = raw_rx.recv() => {
									if next_event.is_none() {
										break 'watcher_loop;
									}
									// Новое событие — сбрасываем таймер debounce.
									continue;
								}

								_ = tokio::time::sleep(DEBOUNCE_DELAY) => {
									log::info!("Build triggered for '{}'", dir);
									let _ = event_tx.send(BuildEvent {
										dir: dir.clone(),
									});
									break;
								}
							}
						}
					}
				}
			}

			drop(watcher);
			log::info!("Watcher for '{}' stopped", dir);
		});

		Self {
			_handle: handle,
			stop_tx,
		}
	}

	/// Остановить watcher
	pub fn stop(self) {
		let _ = self.stop_tx.send(());
	}
}

fn should_watch_path(path: &Path, extensions: &HashSet<String>) -> bool {
	// Игнорируем системные и служебные директории
	if path.components().any(|component| {
		let name = component.as_os_str().to_string_lossy();
		name == "node_modules" || name == ".git" || name == "target"
	}) {
		return false;
	}

	// Проверяем расширение файла
	path.extension()
		.and_then(|ext| ext.to_str())
		.map(|ext| extensions.contains(&ext.to_ascii_lowercase()))
		.unwrap_or(false)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	#[test]
	fn test_extension_filter() {
		let extensions: HashSet<String> = ["js", "ts"].into_iter().map(String::from).collect();

		assert!(should_watch_path(&PathBuf::from("main.js"), &extensions));
		assert!(should_watch_path(&PathBuf::from("src/app.ts"), &extensions));
		assert!(!should_watch_path(&PathBuf::from("style.css"), &extensions));
	}

	#[test]
	fn test_ignore_node_modules() {
		let extensions: HashSet<String> = ["js"].into_iter().map(String::from).collect();

		assert!(!should_watch_path(
			&PathBuf::from("node_modules/pkg/index.js"),
			&extensions
		));
	}

	#[test]
	fn test_extension_case_insensitive() {
		let extensions: HashSet<String> = ["js"].into_iter().map(String::from).collect();

		assert!(should_watch_path(&PathBuf::from("main.JS"), &extensions));
	}
}
