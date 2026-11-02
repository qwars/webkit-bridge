use anyhow::{Context, Result};
use log::{debug, error, info};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

use crate::deps;

/// Менеджер daemon subprocess
pub struct DaemonManager {
	child: Option<Child>,
	socket_path: PathBuf,
}

impl DaemonManager {
	/// Создать и запустить daemon
	pub async fn start(log_tx: mpsc::UnboundedSender<String>) -> Result<Self> {
		let daemon_path = deps::find_ewd_daemon()?;
		let socket_path = Self::generate_socket_path();

		info!("Starting daemon: {:?}", daemon_path);
		info!("Socket path: {:?}", socket_path);

		// !!! ДОБАВЛЕНО: Тестовое сообщение для проверки канала логов !!!
		let _ = log_tx.send("ewd-tui: daemon started".to_string());

		let mut child = Command::new(&daemon_path)
			.env("EWD_SOCKET_PATH", &socket_path)
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::piped())
			.spawn()
			.context("Failed to spawn ewd-daemon")?;

		// Перехватить stderr в отдельную задачу
		if let Some(stderr) = child.stderr.take() {
			let log_tx_clone = log_tx.clone();
			tokio::spawn(async move {
				let reader = BufReader::new(stderr);
				let mut lines = reader.lines();
				while let Ok(Some(line)) = lines.next_line().await {
					let _ = log_tx_clone.send(line);
				}
			});
		}

		// Дождаться готовности (проверить socket existence)
		Self::wait_for_socket(&socket_path).await?;

		info!("Daemon started successfully");

		Ok(Self {
			child: Some(child),
			socket_path,
		})
	}

	/// Сгенерировать уникальный путь сокета
	fn generate_socket_path() -> PathBuf {
		let runtime_dir = std::env::var("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|_| {
			// Fallback для систем без XDG_RUNTIME_DIR
			PathBuf::from("/tmp")
		});

		let pid = std::process::id();
		runtime_dir.join(format!("ewd-tui-{}.sock", pid))
	}

	/// Дождаться появления socket файла
	async fn wait_for_socket(socket_path: &PathBuf) -> Result<()> {
		let timeout_duration = Duration::from_secs(5);

		timeout(timeout_duration, async {
			loop {
				if socket_path.exists() {
					debug!("Socket file appeared: {:?}", socket_path);
					break;
				}
				tokio::time::sleep(Duration::from_millis(100)).await;
			}
		})
		.await
		.map_err(|_| anyhow::anyhow!("Timeout waiting for daemon socket"))?;

		Ok(())
	}

	/// Получить путь сокета
	pub fn socket_path(&self) -> &PathBuf {
		&self.socket_path
	}

	/// Остановить daemon
	pub async fn stop(&mut self) -> Result<()> {
		if let Some(mut child) = self.child.take() {
			info!("Stopping daemon");

			// Послать SIGTERM
			if let Err(e) = child.kill().await {
				error!("Failed to kill daemon: {}", e);
			}

			// Дождаться завершения с timeout
			let wait_result = timeout(Duration::from_secs(3), child.wait()).await;

			match wait_result {
				Ok(Ok(status)) => {
					info!("Daemon exited with status: {}", status);
				}
				Ok(Err(e)) => {
					error!("Error waiting for daemon: {}", e);
				}
				Err(_) => {
					error!("Timeout waiting for daemon to exit, forcing kill");
					let _ = child.kill().await;
				}
			}
		}

		// Удалить socket файл
		if self.socket_path.exists() {
			if let Err(e) = std::fs::remove_file(&self.socket_path) {
				error!("Failed to remove socket file: {}", e);
			}
		}

		Ok(())
	}
}

impl Drop for DaemonManager {
	fn drop(&mut self) {
		// Синхронная очистка при drop (на случай если stop не был вызван)
		if self.socket_path.exists() {
			let _ = std::fs::remove_file(&self.socket_path);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_generate_socket_path() {
		let path = DaemonManager::generate_socket_path();
		assert!(path.to_str().unwrap().contains("ewd-tui-"));
		assert!(path.to_str().unwrap().ends_with(".sock"));
	}

	#[test]
	fn test_socket_path_unique() {
		let path1 = DaemonManager::generate_socket_path();
		let path2 = DaemonManager::generate_socket_path();
		assert_eq!(path1, path2);
	}
}
