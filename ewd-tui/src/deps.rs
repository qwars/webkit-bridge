use anyhow::{bail, Context, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn check_dependencies() -> Result<()> {
	check_ewd_daemon()?;
	check_esbuild()?;
	Ok(())
}

pub fn find_ewd_daemon() -> Result<PathBuf> {
	if let Ok(path) = std::env::var("EWD_DAEMON_PATH") {
		let path = PathBuf::from(path);
		if is_executable_file(&path) {
			return Ok(path);
		}
	}

	let candidates = [
		"./target/release/ewd-daemon",
		"./target/debug/ewd-daemon",
		"../target/release/ewd-daemon",
		"../target/debug/ewd-daemon",
	];

	for candidate in &candidates {
		let path = PathBuf::from(candidate);
		if is_executable_file(&path) {
			return Ok(path);
		}
	}

	if let Ok(output) = Command::new("which").arg("ewd-daemon").output() {
		if output.status.success() {
			let path_str = String::from_utf8_lossy(&output.stdout);
			let path = PathBuf::from(path_str.trim());
			if is_executable_file(&path) {
				return Ok(path);
			}
		}
	}

	bail!(
		"ewd-daemon не найден или не является исполняемым файлом.\n\
         \n\
         Проверенные пути:\n  \
           $EWD_DAEMON_PATH\n  \
           ./target/release/ewd-daemon\n  \
           ./target/debug/ewd-daemon\n  \
           ../target/release/ewd-daemon\n  \
           ../target/debug/ewd-daemon\n  \
           $PATH\n\
         \n\
         Как исправить:\n  \
           Собрать:        cargo build -p ewd-daemon\n  \
           Указать путь:   EWD_DAEMON_PATH=/path/to/ewd-daemon"
	)
}

fn is_executable_file(path: &Path) -> bool {
	if !path.is_file() {
		return false;
	}
	match std::fs::metadata(path) {
		Ok(meta) => meta.permissions().mode() & 0o111 != 0,
		Err(_) => false,
	}
}

fn check_ewd_daemon() -> Result<()> {
	find_ewd_daemon().context("ewd-daemon обязателен для работы ewd-tui")?;
	Ok(())
}

fn check_esbuild() -> Result<()> {
	let output = Command::new("which")
		.arg("esbuild")
		.output()
		.context("Не удалось выполнить 'which esbuild'")?;

	if !output.status.success() {
		bail!(
			"esbuild не найден в PATH.\n\
             \n\
             Как исправить:\n  \
               Debian/Ubuntu: sudo apt install esbuild\n  \
               Fedora:        sudo dnf install esbuild\n  \
               npm:           npm install -g esbuild"
		);
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::env;
	use std::fs;
	use std::os::unix::fs::PermissionsExt;
	use tempfile::TempDir;

	#[test]
	fn test_find_ewd_daemon_from_env() {
		let _lock = crate::test_utils::env_mutex().lock().unwrap_or_else(|e| e.into_inner());
		let temp_dir = TempDir::new().unwrap();
		let daemon_path = temp_dir.path().join("ewd-daemon");
		fs::write(&daemon_path, "").unwrap();
		fs::set_permissions(&daemon_path, fs::Permissions::from_mode(0o755)).unwrap();

		env::set_var("EWD_DAEMON_PATH", daemon_path.to_str().unwrap());
		let result = find_ewd_daemon();
		env::remove_var("EWD_DAEMON_PATH");

		assert!(result.is_ok());
		assert_eq!(result.unwrap(), daemon_path);
	}

	#[test]
	fn test_find_ewd_daemon_env_non_executable() {
		let _lock = crate::test_utils::env_mutex().lock().unwrap_or_else(|e| e.into_inner());
		let temp_dir = TempDir::new().unwrap();
		let daemon_path = temp_dir.path().join("ewd-daemon");
		fs::write(&daemon_path, "").unwrap();
		fs::set_permissions(&daemon_path, fs::Permissions::from_mode(0o644)).unwrap();

		env::set_var("EWD_DAEMON_PATH", daemon_path.to_str().unwrap());
		let _ = find_ewd_daemon();
		env::remove_var("EWD_DAEMON_PATH");
	}

	#[test]
	fn test_is_executable_file_true() {
		let temp_dir = TempDir::new().unwrap();
		let file_path = temp_dir.path().join("executable");
		fs::write(&file_path, "").unwrap();
		fs::set_permissions(&file_path, fs::Permissions::from_mode(0o755)).unwrap();

		assert!(is_executable_file(&file_path));
	}

	#[test]
	fn test_is_executable_file_false_not_executable() {
		let temp_dir = TempDir::new().unwrap();
		let file_path = temp_dir.path().join("non-executable");
		fs::write(&file_path, "").unwrap();
		fs::set_permissions(&file_path, fs::Permissions::from_mode(0o644)).unwrap();

		assert!(!is_executable_file(&file_path));
	}

	#[test]
	fn test_is_executable_file_false_not_exists() {
		let path = PathBuf::from("/nonexistent/path/12345");
		assert!(!is_executable_file(&path));
	}

	#[test]
	fn test_is_executable_file_false_directory() {
		let temp_dir = TempDir::new().unwrap();
		assert!(!is_executable_file(&temp_dir.path().to_path_buf()));
	}

	#[test]
	fn test_check_esbuild_missing() {
		let result = check_esbuild();
		assert!(result.is_ok() || result.is_err());
	}
}
