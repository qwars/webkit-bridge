use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::projects::{bundle_path, entry_path, Project};

#[derive(Debug, Clone)]
pub struct BuildOutput {
	pub success: bool,
	pub stdout: String,
	pub stderr: String,
	pub bundle_path: PathBuf,
}

pub fn build_esbuild_command(project: &Project) -> Result<(String, Vec<String>)> {
	let entry = entry_path(project)?;
	let bundle = bundle_path(&project.dir)?;

	let cmd = "esbuild".to_string();
	let args = vec![
		entry.to_string_lossy().to_string(),
		"--bundle".to_string(),
		"--color=true".to_string(),
		format!("--outfile={}", bundle.to_string_lossy()),
	];

	Ok((cmd, args))
}

/// Запускает esbuild как дочерний процесс с piped stdout/stderr
pub async fn spawn_esbuild(project: &Project) -> Result<tokio::process::Child> {
	let entry = entry_path(project)?;
	let bundle = bundle_path(&project.dir)?;

	let child = Command::new("esbuild")
		.arg(entry.to_string_lossy().to_string())
		.arg("--bundle")
		.arg("--color=true")
		.arg(format!("--outfile={}", bundle.to_string_lossy()))
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.context("Failed to spawn esbuild")?;

	Ok(child)
}

/// Читает stdout/stderr дочернего процесса и ждёт его завершения
pub async fn collect_build_output(child: &mut tokio::process::Child, bundle_path: PathBuf) -> Result<BuildOutput> {
	let stdout = child.stdout.take().context("Failed to take stdout")?;
	let stderr = child.stderr.take().context("Failed to take stderr")?;

	let mut stdout_reader = BufReader::new(stdout);
	let mut stderr_reader = BufReader::new(stderr);

	// Читаем stdout и stderr параллельно
	let stdout_task = tokio::spawn(async move {
		let mut lines = String::new();
		let mut line = String::new();
		while stdout_reader.read_line(&mut line).await? > 0 {
			lines.push_str(&line);
			line.clear();
		}
		Ok::<String, std::io::Error>(lines)
	});

	let stderr_task = tokio::spawn(async move {
		let mut lines = String::new();
		let mut line = String::new();
		while stderr_reader.read_line(&mut line).await? > 0 {
			lines.push_str(&line);
			line.clear();
		}
		Ok::<String, std::io::Error>(lines)
	});

	// Ждём завершения процесса
	let status = child.wait().await.context("Failed to wait for esbuild")?;

	// Ждём завершения чтения stdout/stderr
	let stdout_result = stdout_task.await.context("stdout task failed")??;
	let stderr_result = stderr_task.await.context("stderr task failed")??;

	Ok(BuildOutput {
		success: status.success(),
		stdout: stdout_result,
		stderr: stderr_result,
		bundle_path,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::projects::ProjectConfig;

	fn mock_project() -> Project {
		Project {
			dir: "test-project".to_string(),
			config: ProjectConfig {
				name: "Test".to_string(),
				url: "https://example.com".to_string(),
				entry: "main.js".to_string(),
				watch_extensions: vec!["js".into()],
			},
		}
	}

	#[test]
	fn test_build_esbuild_command() {
		let project = mock_project();
		let (cmd, args) = build_esbuild_command(&project).unwrap();
		assert_eq!(cmd, "esbuild");
		assert!(args.contains(&"--bundle".to_string()));
		assert!(args.contains(&"--color=true".to_string()));
	}
}
