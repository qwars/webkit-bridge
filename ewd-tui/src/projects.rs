use anyhow::{bail, Context, Result};
use log::debug;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

/// Конфигурация проекта (настройки отображения и сборки)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectConfig {
	pub name: String,
	pub url: String,
	pub entry: String,
	#[serde(default = "default_watch_extensions")]
	pub watch_extensions: Vec<String>,
}

fn default_watch_extensions() -> Vec<String> {
	vec![
		"js".to_string(),
		"ts".to_string(),
		"jsx".to_string(),
		"tsx".to_string(),
		"json".to_string(),
		"json5".to_string(),
	]
}

/// Проект = директория + конфигурация
#[derive(Debug, Clone)]
pub struct Project {
	pub dir: String,
	pub config: ProjectConfig,
}

pub fn config_dir() -> Result<PathBuf> {
	let base = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
	Ok(base.join("ewd-tui"))
}

pub fn projects_dir() -> Result<PathBuf> {
	Ok(config_dir()?.join("projects"))
}

pub fn bundles_dir() -> Result<PathBuf> {
	Ok(config_dir()?.join("bundles"))
}

pub fn validate_dir_name(dir: &str) -> Result<()> {
	if dir.is_empty() {
		bail!("Имя каталога не может быть пустым");
	}
	if dir.len() > 255 {
		bail!("Имя каталога не может быть длиннее 255 символов");
	}
	if dir.contains('/') || dir.contains('\\') || dir.contains("..") {
		bail!("Имя каталога не может содержать '/', '\\' или '..'");
	}
	Ok(())
}

const VALID_ENTRY_EXTENSIONS: &[&str] = &["js", "mjs", "cjs", "ts", "tsx", "mts", "cts", "jsx", "css", "json"];

pub fn validate_entry_extension(entry: &str) -> Result<()> {
	let extension = entry
		.rsplit('.')
		.next()
		.ok_or_else(|| anyhow::anyhow!("У файла нет расширения: {}", entry))?;

	let extension_lower = extension.to_lowercase();
	if VALID_ENTRY_EXTENSIONS.contains(&extension_lower.as_str()) {
		Ok(())
	} else {
		bail!(
			"Недопустимое расширение файла: .{}. Допустимые: {}",
			extension,
			VALID_ENTRY_EXTENSIONS
				.iter()
				.map(|e| format!(".{}", e))
				.collect::<Vec<_>>()
				.join(", ")
		)
	}
}

pub fn project_dir(dir: &str) -> Result<PathBuf> {
	validate_dir_name(dir)?;
	Ok(projects_dir()?.join(dir))
}

pub fn project_config_path(dir: &str) -> Result<PathBuf> {
	Ok(project_dir(dir)?.join("config.toml"))
}

pub fn bundle_path(dir: &str) -> Result<PathBuf> {
	validate_dir_name(dir)?;
	Ok(bundles_dir()?.join(format!("{}.js", dir)))
}

pub fn entry_path(project: &Project) -> Result<PathBuf> {
	Ok(project_dir(&project.dir)?.join(&project.config.entry))
}

pub fn ensure_dirs() -> Result<()> {
	let config = config_dir()?;
	let projects = projects_dir()?;
	let bundles = bundles_dir()?;

	if !config.exists() {
		fs::create_dir_all(&config).with_context(|| format!("Failed to create config dir: {:?}", config))?;
	}
	if !projects.exists() {
		fs::create_dir_all(&projects).with_context(|| format!("Failed to create projects dir: {:?}", projects))?;
	}
	if !bundles.exists() {
		fs::create_dir_all(&bundles).with_context(|| format!("Failed to create bundles dir: {:?}", bundles))?;
	}

	debug!("Config directories ensured");
	Ok(())
}

pub fn project_exists(dir: &str) -> bool {
	if let Ok(path) = project_config_path(dir) {
		path.exists()
	} else {
		false
	}
}

pub fn get_project_mtime(dir: &str) -> Result<SystemTime> {
	let config_path = project_config_path(dir)?;
	let metadata =
		fs::metadata(&config_path).with_context(|| format!("Failed to get metadata for {:?}", config_path))?;
	metadata
		.modified()
		.with_context(|| format!("Failed to get modification time for {:?}", config_path))
}

/// Получить время модификации бандла
/// Возвращает Err, если бандл не существует
pub fn get_bundle_mtime(dir: &str) -> Result<SystemTime> {
	let bundle = bundle_path(dir)?;
	let metadata = fs::metadata(&bundle).with_context(|| format!("Failed to get metadata for {:?}", bundle))?;
	metadata
		.modified()
		.with_context(|| format!("Failed to get modification time for {:?}", bundle))
}

/// Получить максимальное время модификации исходников проекта
/// Рекурсивно обходит директорию, исключая служебные папки
pub fn get_sources_mtime(dir: &str) -> Result<SystemTime> {
	let project_path = project_dir(dir)?;
	let mut max_time = SystemTime::UNIX_EPOCH;

	fn walk_dir(path: &std::path::Path, max_time: &mut SystemTime) -> Result<()> {
		for entry in fs::read_dir(path)? {
			let entry = entry?;
			let path = entry.path();

			// Пропускаем служебные директории
			if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
				if name == "node_modules" || name == ".git" || name == "target" || name == "bundles" {
					continue;
				}
			}

			if path.is_dir() {
				walk_dir(&path, max_time)?;
			} else if path.is_file() {
				if let Ok(metadata) = fs::metadata(&path) {
					if let Ok(modified) = metadata.modified() {
						if modified > *max_time {
							*max_time = modified;
						}
					}
				}
			}
		}
		Ok(())
	}

	walk_dir(&project_path, &mut max_time)?;
	Ok(max_time)
}

/// Форматирует время как "2ч назад", "вчера", "3 дня назад" и т.д.
pub fn format_mtime(mtime: SystemTime) -> String {
	let now = SystemTime::now();
	let duration = match now.duration_since(mtime) {
		Ok(d) => d,
		Err(_) => return "только что".to_string(),
	};

	let secs = duration.as_secs();
	if secs < 60 {
		return "только что".to_string();
	}

	let mins = secs / 60;
	if mins < 60 {
		return format!("{} мин назад", mins);
	}

	let hours = mins / 60;
	if hours < 24 {
		return format!("{} ч назад", hours);
	}

	let days = hours / 24;
	if days == 1 {
		return "вчера".to_string();
	}
	if days < 7 {
		return format!("{} дн назад", days);
	}

	let weeks = days / 7;
	if weeks < 4 {
		return format!("{} нед назад", weeks);
	}

	let months = days / 30;
	if months < 12 {
		return format!("{} мес назад", months);
	}

	let years = days / 365;
	format!("{} г назад", years)
}

pub fn list_projects() -> Result<Vec<(Project, SystemTime)>> {
	ensure_dirs()?;
	let projects_path = projects_dir()?;

	let mut projects = Vec::new();
	for entry in
		fs::read_dir(&projects_path).with_context(|| format!("Failed to read projects dir: {:?}", projects_path))?
	{
		let entry = entry?;
		if entry.file_type()?.is_dir() {
			let dir_name = entry.file_name().to_string_lossy().to_string();
			let config_path = entry.path().join("config.toml");

			if config_path.exists() {
				if let Ok(content) = fs::read_to_string(&config_path) {
					if let Ok(config) = toml::from_str::<ProjectConfig>(&content) {
						if let Ok(mtime) = get_project_mtime(&dir_name) {
							projects.push((Project { dir: dir_name, config }, mtime));
						}
					}
				}
			}
		}
	}

	// Сортируем по времени изменения (новые в начало)
	projects.sort_by(|a, b| b.1.cmp(&a.1));

	Ok(projects)
}

pub fn save_project(project: &Project) -> Result<()> {
	validate_dir_name(&project.dir)?;

	let project_path = project_dir(&project.dir)?;
	if !project_path.exists() {
		fs::create_dir_all(&project_path)
			.with_context(|| format!("Failed to create project dir: {:?}", project_path))?;
	}

	let config_path = project_config_path(&project.dir)?;
	let content = toml::to_string_pretty(&project.config).context("Failed to serialize project config")?;

	fs::write(&config_path, content).with_context(|| format!("Failed to write config: {:?}", config_path))?;

	debug!("Project '{}' (dir: '{}') saved", project.config.name, project.dir);
	Ok(())
}

pub fn delete_project(dir: &str) -> Result<()> {
	let project_path = project_dir(dir)?;
	if !project_path.exists() {
		bail!("Проект с каталогом '{}' не найден", dir);
	}

	fs::remove_dir_all(&project_path).with_context(|| format!("Failed to delete project dir: {:?}", project_path))?;

	if let Ok(bundle) = bundle_path(dir) {
		if bundle.exists() {
			let _ = fs::remove_file(&bundle);
		}
	}

	debug!("Project (dir: '{}') deleted", dir);
	Ok(())
}

pub fn create_project(name: &str, dir: &str, url: &str, entry: &str) -> Result<Project> {
	validate_entry_extension(entry)?;
	validate_dir_name(dir)?;

	if name.is_empty() {
		bail!("Название проекта не может быть пустым");
	}
	if name.chars().count() > 255 {
		bail!("Название проекта не может быть длиннее 255 символов");
	}

	if url.is_empty() {
		bail!("URL не может быть пустым");
	}
	if let Err(e) = url::Url::parse(url) {
		bail!("Невалидный URL: {}", e);
	}

	if entry.is_empty() {
		bail!("Entry point не может быть пустым");
	}

	if project_exists(dir) {
		bail!("Проект с каталогом '{}' уже существует", dir);
	}

	let project = Project {
		dir: dir.to_string(),
		config: ProjectConfig {
			name: name.to_string(),
			url: url.to_string(),
			entry: entry.to_string(),
			watch_extensions: default_watch_extensions(),
		},
	};

	save_project(&project)?;

	let entry_file = entry_path(&project)?;
	if !entry_file.exists() {
		if let Some(parent) = entry_file.parent() {
			if !parent.exists() {
				fs::create_dir_all(parent)
					.with_context(|| format!("Failed to create entry parent dir: {:?}", parent))?;
			}
		}
		let template = format!(
			"// Entry point: {}\nconsole.log('Hello from {}');\n",
			project.config.entry, project.config.name
		);
		fs::write(&entry_file, template).with_context(|| format!("Failed to create entry file: {:?}", entry_file))?;
		debug!("Created entry file: {:?}", entry_file);
	}

	debug!("Project '{}' (dir: '{}') created", name, dir);
	Ok(project)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::env;
	use tempfile::TempDir;

	fn setup_temp_config() -> TempDir {
		let temp = TempDir::new().unwrap();
		let config_dir = temp.path().join(".config");
		fs::create_dir_all(&config_dir).unwrap();
		env::set_var("HOME", temp.path());
		env::set_var("XDG_CONFIG_HOME", &config_dir);
		temp
	}

	#[test]
	fn test_validate_dir_name_valid() {
		assert!(validate_dir_name("my-project").is_ok());
		assert!(validate_dir_name("project123").is_ok());
		assert!(validate_dir_name("test_dir").is_ok());
	}

	#[test]
	fn test_validate_dir_name_invalid() {
		assert!(validate_dir_name("").is_err());
		assert!(validate_dir_name(&"a".repeat(256)).is_err());
		assert!(validate_dir_name("my/project").is_err());
		assert!(validate_dir_name("my\\project").is_err());
		assert!(validate_dir_name("my..project").is_err());
	}

	#[test]
	fn test_project_config_roundtrip() {
		let _lock = crate::test_utils::env_mutex().lock().unwrap_or_else(|e| e.into_inner());
		let _temp = setup_temp_config();
		ensure_dirs().unwrap();

		let project = Project {
			dir: "my-cool-project".to_string(),
			config: ProjectConfig {
				name: "Мой крутой проект".to_string(),
				url: "https://example.com".to_string(),
				entry: "src/main.js".to_string(),
				watch_extensions: default_watch_extensions(),
			},
		};

		save_project(&project).unwrap();

		let config_path = project_config_path("my-cool-project").unwrap();
		let content = fs::read_to_string(&config_path).unwrap();
		let loaded: ProjectConfig = toml::from_str(&content).unwrap();
		assert_eq!(project.config, loaded);
	}

	#[test]
	fn test_list_projects() {
		let _lock = crate::test_utils::env_mutex().lock().unwrap_or_else(|e| e.into_inner());
		let _temp = setup_temp_config();
		ensure_dirs().unwrap();

		let project = Project {
			dir: "alpha-dir".to_string(),
			config: ProjectConfig {
				name: "Alpha".to_string(),
				url: "https://example.com".to_string(),
				entry: "main.js".to_string(),
				watch_extensions: default_watch_extensions(),
			},
		};
		save_project(&project).unwrap();

		let projects = list_projects().unwrap();
		assert!(projects.iter().any(|p| p.0.dir == "alpha-dir"));
	}

	#[test]
	fn test_create_and_delete_project() {
		let _lock = crate::test_utils::env_mutex().lock().unwrap_or_else(|e| e.into_inner());
		let _temp = setup_temp_config();
		ensure_dirs().unwrap();

		let unique_dir = format!(
			"temp-proj-{}",
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_nanos()
		);

		let project = create_project("Temp Proj", &unique_dir, "https://example.com", "main.js").unwrap();

		assert!(project_exists(&unique_dir));

		let entry_file = entry_path(&project).unwrap();
		assert!(entry_file.exists());

		delete_project(&unique_dir).unwrap();
		assert!(!project_exists(&unique_dir));
	}

	#[test]
	fn test_create_with_invalid_dir_fails() {
		let _lock = crate::test_utils::env_mutex().lock().unwrap_or_else(|e| e.into_inner());
		let _temp = setup_temp_config();
		ensure_dirs().unwrap();

		let result = create_project("Bad Proj", "bad/proj", "https://example.com", "main.js");
		assert!(result.is_err());
	}

	#[test]
	fn test_entry_path() {
		let _lock = crate::test_utils::env_mutex().lock().unwrap_or_else(|e| e.into_inner());
		let _temp = setup_temp_config();

		let project = Project {
			dir: "test-proj".to_string(),
			config: ProjectConfig {
				name: "Test".to_string(),
				url: "https://example.com".to_string(),
				entry: "main.js".to_string(),
				watch_extensions: default_watch_extensions(),
			},
		};

		let path = entry_path(&project).unwrap();
		assert!(path.to_str().unwrap().contains("test-proj"));
		assert!(path.to_str().unwrap().ends_with("main.js"));
	}
	#[test]
	fn test_validate_entry_extension_valid() {
		assert!(validate_entry_extension("main.js").is_ok());
		assert!(validate_entry_extension("main.ts").is_ok());
		assert!(validate_entry_extension("main.tsx").is_ok());
		assert!(validate_entry_extension("main.jsx").is_ok());
		assert!(validate_entry_extension("main.mjs").is_ok());
		assert!(validate_entry_extension("main.cjs").is_ok());
		assert!(validate_entry_extension("main.css").is_ok());
		assert!(validate_entry_extension("main.json").is_ok());
		assert!(validate_entry_extension("src/main.ts").is_ok());
		assert!(validate_entry_extension("main.JS").is_ok()); // case-insensitive
	}

	#[test]
	fn test_validate_entry_extension_invalid() {
		assert!(validate_entry_extension("main.j").is_err());
		assert!(validate_entry_extension("main.txt").is_err());
		assert!(validate_entry_extension("main.py").is_err());
		assert!(validate_entry_extension("main.html").is_err());
		assert!(validate_entry_extension("main").is_err()); // нет расширения
	}

	#[test]
	fn test_create_with_invalid_entry_extension_fails() {
		let _lock = crate::test_utils::env_mutex().lock().unwrap_or_else(|e| e.into_inner());
		let _temp = setup_temp_config();

		let result = create_project("test", "test-dir", "https://example.com", "in.j");
		assert!(result.is_err());
		let err_msg = result.unwrap_err().to_string();
		assert!(
			err_msg.contains("Недопустимое расширение"),
			"Unexpected error: {}",
			err_msg
		);
	}
}
