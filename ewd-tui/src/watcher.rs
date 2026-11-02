use log::{debug, warn};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

use crate::projects::Project;

#[derive(Debug, Clone)]
pub struct BuildEvent {
	pub dir: String,
}

/// Обёртка над notify::Watcher, которая отслеживает изменения в директории проекта
/// и отправляет BuildEvent при изменении файлов с нужными расширениями.
pub struct ProjectWatcher {
	_watcher: Arc<Mutex<RecommendedWatcher>>,
	stop_tx: mpsc::UnboundedSender<()>,
}

impl ProjectWatcher {
	/// Запускает watcher для проекта.
	/// `bundle_path` — путь к bundle файлу, изменения которого нужно игнорировать.
	pub fn start(project: Project, event_tx: mpsc::UnboundedSender<BuildEvent>, bundle_path: PathBuf) -> Self {
		let dir = project.dir.clone();
		let extensions: HashSet<String> = project
			.config
			.watch_extensions
			.iter()
			.map(|e| e.to_lowercase())
			.collect();

		// Канал для остановки watcher
		let (stop_tx, mut stop_rx) = mpsc::unbounded_channel::<()>();

		// Канал для событий от notify
		let (notify_tx, mut notify_rx) = mpsc::unbounded_channel::<notify::Result<Event>>();

		let config = Config::default().with_poll_interval(Duration::from_millis(500));

		let watcher = Arc::new(Mutex::new(
			RecommendedWatcher::new(
				move |res| {
					let _ = notify_tx.send(res);
				},
				config,
			)
			.expect("Failed to create watcher"),
		));

		// Определяем директорию проекта
		let project_dir = match crate::projects::project_dir(&dir) {
			Ok(d) => d,
			Err(e) => {
				warn!("Failed to get project dir for '{}': {}", dir, e);
				return Self {
					_watcher: watcher,
					stop_tx,
				};
			}
		};

		// Захватываем лок в отдельном блоке, чтобы guard не жил до конца if let
		let watch_result = {
			let mut guard = watcher.lock().unwrap();
			guard.watch(&project_dir, RecursiveMode::Recursive)
		};

		if let Err(e) = watch_result {
			warn!("Failed to watch directory {:?}: {}", project_dir, e);
			return Self {
				_watcher: watcher,
				stop_tx,
			};
		}

		log::info!("Watcher started for project '{}' at {:?}", dir, project_dir);

		// Фоновая задача для обработки событий
		tokio::spawn(async move {
			loop {
				tokio::select! {
					_ = stop_rx.recv() => {
						log::info!("Watcher for '{}' stopped", dir);
						break;
					}
					Some(event_result) = notify_rx.recv() => {
						match event_result {
							Ok(event) => {
								if should_trigger_build(&event, &extensions, &bundle_path) {
									debug!("Build trigger event for '{}': {:?}", dir, event.kind);
									let _ = event_tx.send(BuildEvent { dir: dir.clone() });
								}
							}
							Err(e) => {
								warn!("Watcher error for '{}': {}", dir, e);
							}
						}
					}
				}
			}
		});

		Self {
			_watcher: watcher,
			stop_tx,
		}
	}

	/// Останавливает watcher
	pub fn stop(self) {
		let _ = self.stop_tx.send(());
	}
}

/// Проверяет, является ли путь bundle-файлом сборки.
/// Сравнивает как сырые пути, так и канонические (симлинки, относительные пути).
fn is_bundle_file(path: &Path, bundle_path: &Path) -> bool {
	if path == bundle_path {
		return true;
	}
	match (path.canonicalize(), bundle_path.canonicalize()) {
		(Ok(a), Ok(b)) => a == b,
		_ => false,
	}
}

/// Проверяет, нужно ли запускать сборку для данного события
fn should_trigger_build(event: &Event, extensions: &HashSet<String>, bundle_path: &Path) -> bool {
	use notify::EventKind;
	match event.kind {
		EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
		_ => return false,
	}

	for path in &event.paths {
		// Игнорируем изменения bundle файла (предотвращает рекурсию сборки)
		if is_bundle_file(path, bundle_path) {
			debug!("Ignoring bundle file change: {:?}", path);
			continue;
		}

		// Игнорируем node_modules и .git
		if path
			.components()
			.any(|c| c.as_os_str() == "node_modules" || c.as_os_str() == ".git")
		{
			continue;
		}

		if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
			if extensions.contains(&ext.to_lowercase()) {
				return true;
			}
		}
	}

	false
}

#[cfg(test)]
mod tests {
	use super::*;
	use notify::event::ModifyKind;
	use notify::EventKind;

	fn make_event(paths: Vec<PathBuf>) -> Event {
		Event {
			kind: EventKind::Modify(ModifyKind::Any),
			paths,
			attrs: Default::default(),
		}
	}

	fn exts(list: &[&str]) -> HashSet<String> {
		list.iter().map(|s| s.to_string()).collect()
	}

	#[test]
	fn test_extension_filter() {
		let extensions = exts(&["js", "ts"]);
		let bundle_path = PathBuf::from("/tmp/bundle.js");

		let event = make_event(vec![PathBuf::from("/project/src/main.js")]);
		assert!(should_trigger_build(&event, &extensions, &bundle_path));

		let event = make_event(vec![PathBuf::from("/project/src/app.ts")]);
		assert!(should_trigger_build(&event, &extensions, &bundle_path));

		let event = make_event(vec![PathBuf::from("/project/src/style.css")]);
		assert!(!should_trigger_build(&event, &extensions, &bundle_path));
	}

	#[test]
	fn test_extension_case_insensitive() {
		let extensions = exts(&["js"]);
		let bundle_path = PathBuf::from("/tmp/bundle.js");

		let event = make_event(vec![PathBuf::from("/project/src/main.JS")]);
		assert!(should_trigger_build(&event, &extensions, &bundle_path));
	}

	#[test]
	fn test_ignore_node_modules() {
		let extensions = exts(&["js"]);
		let bundle_path = PathBuf::from("/tmp/bundle.js");

		let event = make_event(vec![PathBuf::from("/project/node_modules/lib/index.js")]);
		assert!(!should_trigger_build(&event, &extensions, &bundle_path));
	}

	#[test]
	fn test_ignore_bundle_file() {
		let extensions = exts(&["js"]);
		let bundle_path = PathBuf::from("/tmp/bundle.js");

		// Изменение bundle файла должно игнорироваться
		let event = make_event(vec![PathBuf::from("/tmp/bundle.js")]);
		assert!(!should_trigger_build(&event, &extensions, &bundle_path));

		// Изменение исходника в той же директории должно срабатывать
		let event = make_event(vec![PathBuf::from("/tmp/hy.js")]);
		assert!(should_trigger_build(&event, &extensions, &bundle_path));
	}

	#[test]
	fn test_bundle_file_real_path() {
		let dir = tempfile::TempDir::new().unwrap();
		let bundle = dir.path().join("bundle.js");
		let source = dir.path().join("hy.js");
		std::fs::write(&bundle, "// bundle").unwrap();
		std::fs::write(&source, "// source").unwrap();

		let extensions = exts(&["js"]);

		// Событие с каноническим путём к bundle — игнорируем
		let event = make_event(vec![bundle.canonicalize().unwrap()]);
		assert!(!should_trigger_build(&event, &extensions, &bundle));

		// Событие с исходником — срабатывает
		let event = make_event(vec![source.canonicalize().unwrap()]);
		assert!(should_trigger_build(&event, &extensions, &bundle));
	}
}
