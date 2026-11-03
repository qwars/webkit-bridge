mod app;
mod buffer_ops;
mod build;
mod config;
mod create_project;
mod daemon;
mod delete;
mod deps;
mod input_field;
mod ipc;
mod keys;
mod navigation;
mod project_mgmt;
mod projects;
mod search;
mod text_buffer;
mod ui;
mod views;
mod watcher;

#[cfg(test)]
mod test_utils;

use anyhow::{Context, Result};
use log::info;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
	env_logger::init();

	if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
		print_help();
		return;
	}

	if let Err(err) = run().await {
		eprintln!("Ошибка: {:#}", err);
		eprintln!();
		print_help();
		std::process::exit(1);
	}
}

async fn run() -> Result<()> {
	info!("Starting ewd-tui");

	deps::check_dependencies().context("Проверка зависимостей не пройдена")?;
	info!("All dependencies found");

	projects::ensure_dirs().context("Не удалось создать директории конфигурации")?;

	let global_config = config::load().context("Не удалось загрузить глобальный конфиг")?;
	info!("Loaded config with keymap: {:?}", global_config.keymap);

	let (log_tx, log_rx) = mpsc::unbounded_channel();

	let mut daemon_manager = daemon::DaemonManager::start(log_tx)
		.await
		.context("Не удалось запустить ewd-daemon")?;

	info!("Daemon started at {:?}", daemon_manager.socket_path());

	let (ipc_client, network_rx) = match ipc::IpcClient::connect(daemon_manager.socket_path()).await {
		Ok(res) => res,
		Err(e) => {
			let _ = daemon_manager.stop().await;
			return Err(e.context("Не удалось подключиться к ewd-daemon по IPC"));
		}
	};
	let ipc_client = Arc::new(ipc_client);

	info!("Connected to daemon, client_id={}", ipc_client.client_id());

	let mut state = app::AppState::new(log_rx, network_rx)
		.await
		.context("Failed to initialize app state")?;

	state.keymap = global_config.keymap;
	state.set_ipc_client(ipc_client);

	ui::run_ui(state).await?;

	info!("Shutting down");
	daemon_manager.stop().await?;

	Ok(())
}

fn print_help() {
	println!(
		"ewd-tui — терминальный интерфейс для управления веб-проектами

ЗАПУСК
  ewd-tui

ТРЕБОВАНИЯ
  • ewd-daemon должен быть доступен в PATH или указан через EWD_DAEMON_PATH
  • esbuild должен быть установлен:
    - Debian/Ubuntu: sudo apt install esbuild
    - Fedora:        sudo dnf install esbuild
    - npm:           npm install -g esbuild

ГОРЯЧИЕ КЛАВИШИ
  Список проектов:
    ↑/↓     навигация
    Enter   войти в проект
    /       поиск
    n       создать новый проект
    d       удалить проект
    Ctrl+K  переключить keymap (Emacs/Vim/VSCode)
    q       выход

  Внутри проекта:
    Shift+C/N/R/D/L  переключение вкладок (Console/Network/Resources/DOM/Logs)
    Enter            собрать и запустить (или перезапустить)
    Esc              остановить и выйти

  В Console/Logs/Network:
    ↑/↓/←/→          навигация
    Shift+стрелки    выделение
    PageUp/Down      прокрутка страницами
    Home/End         начало/конец буфера
    Ctrl+Space       установить/сбросить якорь (Emacs)
    Alt+W            копировать выделенное (Emacs)
    Alt+Y            копировать выделенное (Vim)
    Ctrl+C           копировать выделенное (VSCode)
    Ctrl+A           выделить всё

ПЕРЕМЕННЫЕ ОКРУЖЕНИЯ
  EWD_DAEMON_PATH   путь к бинарнику ewd-daemon (если не в PATH)
  RUST_LOG          уровень логирования (error, warn, info, debug)

КОНФИГ
  ~/.config/ewd-tui/config.toml — глобальные настройки (keymap)

ДИАГНОСТИКА
  RUST_LOG=debug ewd-tui 2>ewd-tui.log"
	);
}
