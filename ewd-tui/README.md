# ewd-tui

Terminal UI for managing web projects with `ewd-daemon` — a WebKitGTK browser automation daemon.

## Overview

`ewd-tui` is a terminal-based interface that manages web development projects, builds them with `esbuild`, and serves them through `ewd-daemon` (a WebKitGTK browser daemon). It provides project management, file watching, auto-rebuild, a console, and a network inspector — all from the terminal.

## Key Features

- **Project management** — create, delete, and navigate projects via a list view
- **Auto-build** — watches source files and rebuilds with `esbuild` on change
- **Browser integration** — automatically starts `ewd-daemon`, creates a visible browser tab, and loads the project URL
- **Network inspector** — view network requests with timing, status codes, headers, and filtering
- **Console & logs** — real-time output from the build process and daemon logs
- **Multiple keymaps** — Emacs, Vim, and VSCode keybindings (toggle with `Ctrl+K`)
- **Search** — fuzzy search across projects and network events

## Architecture

```
ewd-tui/
├── src/
│   ├── main.rs          # Entry point, startup flow
│   ├── app.rs           # Application state, screens, network events
│   ├── ui.rs            # ratatui rendering, key event dispatch
│   ├── ipc.rs           # JSON-RPC client over Unix socket
│   ├── daemon.rs        # ewd-daemon subprocess management
│   ├── projects.rs      # Project CRUD, config, validation
│   ├── build.rs         # esbuild invocation
│   ├── watcher.rs       # File system watcher (notify)
│   ├── keys.rs          # Keymap definitions (Emacs/Vim/VSCode)
│   ├── config.rs        # Global config (keymap)
│   ├── navigation.rs    # Project list navigation & filtering
│   ├── search.rs        # Project search
│   ├── create_project.rs # Create project form state
│   ├── delete.rs        # Delete project confirmation
│   ├── input_field.rs   # Text input with cursor/select
│   ├── text_buffer.rs   # Scrollable text buffer
│   ├── buffer_ops.rs    # Selection/copy operations
│   ├── project_mgmt.rs  # Build, start, stop project lifecycle
│   ├── views/           # ratatui view implementations
│   │   ├── mod.rs
│   │   ├── projects.rs
│   │   ├── project.rs
│   │   ├── create_project.rs
│   │   ├── confirm_delete.rs
│   │   ├── network_inspect.rs
│   │   ├── console.rs
│   │   ├── codegen.rs
│   │   └── stubs.rs
│   └── test_utils.rs    # Test utilities
└── Cargo.toml
```

### Components

| Component | Description |
|---|---|
| `AppState` | Central state: projects, screens, buffers, IPC client, watchers |
| `IpcClient` | JSON-RPC 2.0 client over Unix socket (async, with reader loop) |
| `DaemonManager` | Spawns `ewd-daemon`, manages lifecycle, cleans up socket |
| `ProjectWatcher` | `notify`-based file watcher, triggers rebuild on source changes |
| `Keymap` | Three keybinding schemes: Emacs, Vim, VSCode |

## System Requirements

- **OS:** Linux (tested on Debian 13)
- **Display:** Terminal with 256-color support (true color recommended)
- **Shell:** Any POSIX-compatible shell

### Required Dependencies

**System packages:**

```bash
# Debian/Ubuntu
sudo apt install esbuild

# Fedora
sudo dnf install esbuild

# npm
npm install -g esbuild
```

**Rust toolchain:**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Installation

### Build from Source

```bash
git clone <repository-url>
cd webkit-bridge
cargo build --release -p ewd-tui
```

Binary will be in `target/release/ewd-tui`.

### Use from Workspace

```bash
cargo run --release -p ewd-tui
```

## Usage

### Start

```bash
ewd-tui
```

On launch, `ewd-tui` will:

1. Check for `ewd-daemon` and `esbuild` in PATH
2. Create config directories (`~/.config/ewd-tui/`)
3. Start `ewd-daemon` as a child process
4. Connect to the daemon via Unix socket
5. Display the projects list

### Create a Project

1. Press `n` on the projects list
2. Fill in the form:
   - **Name** — display name of the project
   - **Dir** — directory name (used as filesystem path under `~/.config/ewd-tui/projects/`)
   - **URL** — the URL the browser tab will load (e.g., `http://localhost:3000`)
   - **Entry** — entry file for esbuild (e.g., `src/main.js`)
3. Press `Enter` to submit, `Esc` to cancel

A `config.toml` is created in `~/.config/ewd-tui/projects/<dir>/` and an entry file is generated if it doesn't exist.

### Enter a Project

1. Navigate the project list with `↑` / `↓`
2. Press `Enter` to enter the project view
3. The project is built with esbuild and opened in a browser tab via `ewd-daemon`

### Switch Tabs

Inside a project view, switch between tabs:

| Key | Tab |
|---|---|
| `Shift+C` | Console (build output) |
| `Shift+N` | Network (requests) |
| `Shift+R` | Resources |
| `Shift+D` | DOM |
| `Shift+L` | Logs (daemon output) |

### Build & Restart

| Key | Action |
|---|---|
| `F5` / `Ctrl+R` | Build and restart the project |
| `Esc` | Exit project view, stop project |

### Delete a Project

1. Select the project in the list
2. Press `d` to initiate deletion
3. Confirm with `y`, cancel with `n` or `Esc`

### Search

- Press `/` on the projects list to enter search mode
- Type to filter projects by name
- Press `Enter` to open the first match, `Esc` to cancel

### Network Inspector

- `Enter` on a network request — open detail view
- `Space` — mark/unmark a request
- `c` — clear all marks
- `g` — generate API file from marked requests
- `/` — filter network events by URL/method
- `↑` / `↓` / `PageUp` / `PageDown` / `Home` / `End` — navigation

## Keymaps

Toggle keymap with `Ctrl+K`. Three layouts are available:

### Global Keys

| Key | Action |
|---|---|
| `q` | Quit |
| `?` | Show help menu |
| `Ctrl+K` | Toggle keymap (Emacs → Vim → VSCode → Emacs) |

### Emacs Keymap

| Context | Key | Action |
|---|---|---|
| Create form | `Ctrl+A` | Move to line start |
| Create form | `Ctrl+E` | Move to line end |
| Create form | `Ctrl+B` / `Ctrl+F` | Move left / right |
| Create form | `Ctrl+W` | Cut word |
| Create form | `Ctrl+Y` | Paste |
| Create form | `Ctrl+K` | Kill line |
| Create form | `Ctrl+U` | Backward kill line |
| Create form | `Alt+B` / `Alt+F` | Move word left / right |
| Create form | `Alt+W` | Copy |
| Create form | `Tab` | Next field |
| Buffer | `Ctrl+Space` | Toggle mark |
| Buffer | `Alt+W` | Copy selection |
| Buffer | `Alt+<` / `Alt+>` | Home / End |
| Buffer | `Ctrl+A` | Select all |

### Vim Keymap

| Context | Key | Action |
|---|---|---|
| Create form | `Alt+B` / `Alt+W` | Move word left / right |
| Create form | `Alt+0` / `Alt+$` | Line start / end |
| Create form | `Alt+D` | Cut word |
| Create form | `Alt+Y` | Copy |
| Create form | `Alt+P` | Paste |
| Buffer | `Alt+H` / `Alt+J` / `Alt+K` / `Alt+L` | Move left / down / up / right |
| Buffer | `Alt+Y` | Copy |
| Buffer | `Alt+G` | End |

### VSCode Keymap

| Context | Key | Action |
|---|---|---|
| Create form | `Ctrl+C` | Copy |
| Create form | `Ctrl+X` | Cut |
| Create form | `Ctrl+V` | Paste |
| Create form | `Ctrl+A` | Select all |
| Create form | `Ctrl+←` / `Ctrl+→` | Move word left / right |
| Create form | `Ctrl+Shift+←` / `Ctrl+Shift+→` | Select word left / right |
| Buffer | `Ctrl+C` | Copy |
| Buffer | `Ctrl+A` | Select all |

## Configuration

### Global Config

Path: `~/.config/ewd-tui/config.toml`

```toml
keymap = "emacs"
```

Available keymap values: `emacs`, `vim`, `vscode`. Default: `emacs`.

### Project Config

Path: `~/.config/ewd-tui/projects/<dir>/config.toml`

```toml
name = "My Project"
url = "http://localhost:3000"
entry = "src/main.js"
watch_extensions = ["js", "ts", "jsx", "tsx", "json", "json5"]
```

- **name** — display name
- **url** — URL loaded in the browser tab
- **entry** — entry file for esbuild
- **watch_extensions** — file extensions that trigger a rebuild (default: js, ts, jsx, tsx, json, json5)

## Directory Structure

```
~/.config/ewd-tui/
├── config.toml          # Global config
├── projects/            # Project configurations
│   └── <dir>/
│       └── config.toml  # Project config
│       └── <entry>      # Entry file (generated if missing)
└── bundles/             # esbuild output
    └── <dir>.js         # Compiled bundle
```

## Troubleshooting

### ewd-daemon Not Found

Ensure `ewd-daemon` is built and in PATH, or set `EWD_DAEMON_PATH`:

```bash
cargo build --release -p ewd-daemon
export EWD_DAEMON_PATH=./target/release/ewd-daemon
ewd-tui
```

### esbuild Not Found

Install esbuild via your package manager (see System Requirements above).

### Terminal Display Issues

- Ensure your terminal supports 256 colors or true color
- Set `TERM=xterm-256color` if colors are incorrect
- Some terminals may need `COLORTERM=truecolor` set

### Socket Already Exists

If the daemon crashed previously, the socket file may linger:

```bash
rm ~/.config/ewd-tui/ewd-tui-<pid>.sock
```

### Debug Logging

```bash
RUST_LOG=debug ewd-tui 2>ewd-tui.log
```

## License

MIT
