use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::keys::Keymap;
use crate::projects::config_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
	#[serde(default)]
	pub keymap: Keymap,
}

impl Default for GlobalConfig {
	fn default() -> Self {
		Self {
			keymap: Keymap::default(),
		}
	}
}

pub fn config_path() -> Result<PathBuf> {
	Ok(config_dir()?.join("config.toml"))
}

pub fn load() -> Result<GlobalConfig> {
	let path = config_path()?;
	if !path.exists() {
		return Ok(GlobalConfig::default());
	}
	let content = fs::read_to_string(&path).with_context(|| format!("Failed to read config: {:?}", path))?;
	let config: GlobalConfig =
		toml::from_str(&content).with_context(|| format!("Failed to parse config: {:?}", path))?;
	Ok(config)
}

pub fn save(config: &GlobalConfig) -> Result<()> {
	let path = config_path()?;
	if let Some(parent) = path.parent() {
		if !parent.exists() {
			fs::create_dir_all(parent).with_context(|| format!("Failed to create config dir: {:?}", parent))?;
		}
	}
	let content = toml::to_string_pretty(config).context("Failed to serialize config")?;
	fs::write(&path, content).with_context(|| format!("Failed to write config: {:?}", path))?;
	Ok(())
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
	fn test_load_default_when_missing() {
		let _lock = crate::test_utils::env_mutex().lock().unwrap_or_else(|e| e.into_inner());
		let _temp = setup_temp_config();
		let config = load().unwrap();
		assert_eq!(config.keymap, Keymap::Emacs);
	}

	#[test]
	fn test_save_and_load_roundtrip() {
		let _lock = crate::test_utils::env_mutex().lock().unwrap_or_else(|e| e.into_inner());
		let _temp = setup_temp_config();
		let config = GlobalConfig { keymap: Keymap::Vim };
		save(&config).unwrap();
		let loaded = load().unwrap();
		assert_eq!(loaded.keymap, Keymap::Vim);
	}
}
