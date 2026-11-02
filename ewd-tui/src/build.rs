use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
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
		"--minify".to_string(),
		"--color=true".to_string(),
		format!("--outfile={}", bundle.to_string_lossy()),
	];

	Ok((cmd, args))
}

pub async fn run_esbuild(project: &Project) -> Result<BuildOutput> {
	let entry = entry_path(project)?;
	let bundle = bundle_path(&project.dir)?;

	let output = Command::new("esbuild")
		.arg(entry.to_string_lossy().to_string())
		.arg("--bundle")
		.arg("--minify")
		.arg("--color=true")
		.arg(format!("--outfile={}", bundle.to_string_lossy()))
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.output()
		.await
		.context("Failed to execute esbuild")?;

	let stdout = String::from_utf8_lossy(&output.stdout).to_string();
	let stderr = String::from_utf8_lossy(&output.stderr).to_string();
	let success = output.status.success();

	Ok(BuildOutput {
		success,
		stdout,
		stderr,
		bundle_path: bundle,
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
		assert!(args.contains(&"--minify".to_string()));
		assert!(args.contains(&"--color=true".to_string()));
	}
}
