# webkit-bridge

A high-performance, lightweight WebKitGTK daemon for headless and visible browser automation, controlled via JSON-RPC 2.0 over Unix sockets.

## Overview

`webkit-bridge` is a Rust workspace providing a daemon (`ewd-daemon`), CLI utility (`wkctl`), terminal UI (`ewd-tui`), and Emacs client (`ewd.el`) for managing WebKitGTK browser instances through a clean JSON-RPC API. Designed for integration with scripts, automation tools, and any JSON-RPC 2.0 compatible client.

## Key Features

- Isolated tab management with automatic cleanup of orphaned tabs
- Client-based access control — each client can only manage tabs it created
- Proxy support (SOCKS5, HTTP) for anonymous browsing
- **Headless and visible modes** with dynamic switching via `show`/`hide` commands
- Push notifications via `tabs_changed` JSON-RPC notifications
- Zero-copy Unix socket IPC for maximum performance
- Graceful shutdown via IPC command
- **Terminal UI** (`ewd-tui`) — project management, auto-build with esbuild, network inspector, console
- **Emacs integration** — full-featured client with tab management, JavaScript evaluation, dashboard

## Architecture

```
webkit-bridge/
├── ewd-core/          # Library crate (reusable API)
├── ewd-daemon/        # Binary crate (standalone daemon)
├── wkctl/             # CLI utility for testing and scripting
├── ewd-tui/           # Terminal UI (ratatui) — project management, network inspector
├── emacs/             # Emacs client (ewd.el)
├── elisp-packages/    # Emacs Lisp packages:
│   ├── ewd/           #   Main EWD client
│   ├── ewd-deepseek/  #   DeepSeek AI integration
│   └── ewd-translate/ #   Translation plugin
├── bash/              # Integration tests
├── docs/              # Documentation (protocol, events)
└── release-pkg/       # Release packaging scripts
```

- `ewd-core`: Shared library with `WebViewManager`, `Tab`, `Profile`, and event system
- `ewd-daemon`: Standalone daemon with GTK main loop + Tokio async runtime
- `wkctl`: CLI client for testing and scripting
- `ewd-tui`: Terminal UI for project management, auto-build with esbuild, network inspector
- `emacs/`: Emacs Lisp client for seamless integration
- `elisp-packages/`: Additional Emacs Lisp packages (DeepSeek AI, translation)
- `bash/`: Integration test suite
- `docs/`: Protocol specification and event reference

## System Requirements

### Supported Platforms

- OS: Linux (tested on Debian 13, should work on any modern Linux distribution)
- Display: Wayland or X11
- GPU: Any GPU with proper drivers (tested on AMDGPU, Intel, NVIDIA)
- Terminal: 256-color support (true color recommended for `ewd-tui`)

### Required Dependencies

System packages (Debian/Ubuntu example):

```bash
sudo apt install \
  libwebkitgtk-6.0-dev \
  libgtk-4-dev \
  libsoup-3.0-dev \
  libjavascriptcoregtk-6.0-dev \
  pkg-config \
  build-essential
```

For `ewd-tui`:

```bash
sudo apt install esbuild   # or: npm install -g esbuild
```

Rust toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Installation

### Build from Source

```bash
git clone <repository-url>
cd webkit-bridge
cargo build --release
```

Binaries will be in `target/release/`:
- `ewd-daemon` — The main daemon
- `wkctl` — CLI client utility
- `ewd-tui` — Terminal UI

### Install System-wide (Optional)

```bash
sudo cp target/release/ewd-daemon /usr/local/bin/
sudo cp target/release/wkctl /usr/local/bin/
sudo cp target/release/ewd-tui /usr/local/bin/
```

### Build Individual Components

```bash
# Daemon only
cargo build --release -p ewd-daemon

# CLI client only
cargo build --release -p wkctl

# Terminal UI only
cargo build --release -p ewd-tui
```

## Running the Daemon

### Manual Start

```bash
ewd-daemon
# or with proxy:
ewd-daemon --proxy socks5://127.0.0.1:9050
```

The daemon will:
1. Initialize GTK4 and WebKitGTK
2. Create Unix socket at `$XDG_RUNTIME_DIR/webkit-bridge.sock` (default)
3. Listen for JSON-RPC connections
4. Log events to stdout

### Systemd User Service (Recommended)

Create `~/.config/systemd/user/ewd-daemon.service`:

```ini
[Unit]
Description=WebKit Bridge Daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/local/bin/ewd-daemon
Environment="WEBKIT_DISABLE_COMPOSITING_MODE=1"
Environment="WEBKIT_DISABLE_DMABUF_RENDERER=1"
Environment="GDK_BACKEND=wayland"
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

Enable and start:

```bash
systemctl --user daemon-reload
systemctl --user enable ewd-daemon
systemctl --user start ewd-daemon
```

Check status:

```bash
systemctl --user status ewd-daemon
journalctl --user -u ewd-daemon -f
```

## Environment Variables

| Variable                          | Description                                                           |
|-----------------------------------|-----------------------------------------------------------------------|
| `EWD_SOCKET_PATH`                 | Custom socket path (overrides default)                                |
| `XDG_RUNTIME_DIR`                 | Standard XDG runtime directory (used if `EWD_SOCKET_PATH` is not set) |
| `WEBKIT_DISABLE_COMPOSITING_MODE` | Recommended for Wayland/AMDGPU stability                              |
| `WEBKIT_DISABLE_DMABUF_RENDERER`  | Recommended for Wayland/AMDGPU stability                              |
| `GDK_BACKEND`                     | Recommended: `wayland`                                                |
| `EWD_DAEMON_PATH`                 | Custom path to `ewd-daemon` binary (used by `ewd-tui`)                |

Default socket path priority:
1. `EWD_SOCKET_PATH` (if set)
2. `$XDG_RUNTIME_DIR/webkit-bridge.sock`
3. Error if neither is set

## JSON-RPC API Reference

The daemon communicates via JSON-RPC 2.0 over Unix domain sockets using Newline-Delimited JSON (NDJSON) format.

Socket path: `$XDG_RUNTIME_DIR/webkit-bridge.sock` (default)

### Client Identification

Each connection receives a unique `client_id` (UUID). Clients can override this by including `"client_id"` in request params (used by `wkctl` with fixed ID `00000000-0000-0000-0000-000000000000`).

### Methods

#### `create`

Create a new browser tab.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "create",
  "params": {
    "url": "https://example.com",
    "visible": true,
    "client_id": "uuid-string"
  },
  "id": 1
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9"
  },
  "id": 1
}
```

Parameters:
- `url` (string, optional): URL to load. Default: `"about:blank"`
- `visible` (boolean, optional): Show GTK window. Default: `false`
- `client_id` (string, optional): Override client UUID

#### `close`

Close an existing tab. Only the owner client can close its tabs.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "close",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "client_id": "uuid-string"
  },
  "id": 2
}
```

**Response (success):**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 2
}
```

**Response (error — not owner):**
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32000,
    "message": "Not owner of tab: ac93f48c-9b3b-4b64-8852-966d2af207b9"
  },
  "id": 2
}
```

#### `navigate`

Navigate a tab to a new URL. Only the owner client can navigate its tabs.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "navigate",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "url": "https://example.org",
    "client_id": "uuid-string"
  },
  "id": 3
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 3
}
```

#### `reload`

Reload a tab. Only the owner client can reload its tabs.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "reload",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "client_id": "uuid-string"
  },
  "id": 4
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 4
}
```

#### `eval`

Execute JavaScript in a tab context. Only the owner client can evaluate JS in its tabs.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "eval",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "script": "document.title",
    "client_id": "uuid-string"
  },
  "id": 5
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "result": "\"Example Domain\""
  },
  "id": 5
}
```

#### `list`

List all open tabs. Available to all clients (no ownership check).

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "list",
  "params": {
    "client_id": "uuid-string"
  },
  "id": 6
}
```

**Response:**
```json
{
   "jsonrpc": "2.0",
   "result": [
     {
       "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
       "url": "https://example.com",
       "title": "Example Domain",
       "owner": "00000000",
       "visible": false
     },
     {
       "id": "bb91aed9-1d07-46ef-87ae-f4d75689881c",
       "url": "https://google.com",
       "title": "Google",
       "owner": "eff7e05a",
       "visible": true
     }
   ],
   "id": 6
 }
```

Fields:
- `id` (string): Tab UUID
- `url` (string): Current URL
- `title` (string): Page title
- `owner` (string): Short UUID (first 8 chars) of the owner client
- `visible` (boolean): Whether the tab is visible in a GTK window

#### `show`

Show a tab in a GTK window. If the tab is already visible, brings the window to focus. Only the owner client can show its tabs.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "show",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "client_id": "uuid-string"
  },
  "id": 8
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 8
}
```

Use case: Solve CAPTCHAs manually or interact with pages that require user input.

#### `hide`

Hide a tab (convert to headless). The GTK window is closed, but the tab and its WebView remain alive. Only the owner client can hide its tabs.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "hide",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "client_id": "uuid-string"
  },
  "id": 9
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 9
}
```

Use case: After solving a CAPTCHA, hide the window to return to headless mode.

#### `shutdown`

Shutdown the daemon gracefully.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "shutdown",
  "params": {
    "client_id": "uuid-string"
  },
  "id": 7
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 7
}
```

### Notifications

#### `tabs_changed`

Sent when any tab state changes (create, close, navigate, reload, title change, visibility change). This is a notification (no `id` field), not a response.

```json
{
  "jsonrpc": "2.0",
  "method": "tabs_changed",
  "params": {}
}
```

Clients should react by calling `list` to get the updated tab state.

### Error Codes

- `-32600`: Invalid Request
- `-32601`: Method not found
- `-32602`: Invalid params
- `-32000`: Generic error (tab not found, not owner, eval error, etc.)

## Testing with wkctl

The `wkctl` CLI utility provides a convenient way to test the daemon:

```bash
# Create a visible tab
wkctl create --url https://example.com --visible

# Create a headless tab
wkctl create --url https://example.com

# List all tabs
wkctl list

# Show a headless tab in a GTK window
wkctl show --id <tab-id>

# Hide a visible tab (convert to headless)
wkctl hide --id <tab-id>

# Navigate to a new URL
wkctl navigate --id <tab-id> --url https://example.org

# Reload the tab
wkctl reload --id <tab-id>

# Evaluate JavaScript
wkctl eval --id <tab-id> --script 'document.title'

# Close the tab
wkctl close --id <tab-id>

# Shutdown the daemon
wkctl shutdown
```

**Note:** `wkctl` uses a fixed `client_id` (`00000000-0000-0000-0000-000000000000`), so all `wkctl` invocations share ownership of tabs created via CLI.

## ewd-tui — Terminal UI

[`ewd-tui`](ewd-tui/README.md) is a full-featured terminal interface for managing web development projects with `ewd-daemon`. Built with `ratatui`, it provides:

- **Project management** — create, delete, and navigate projects via a list view
- **Auto-build** — watches source files and rebuilds with `esbuild` on change
- **Browser integration** — automatically starts `ewd-daemon`, creates a visible tab, and loads the project URL
- **Network inspector** — view network requests with timing, status codes, headers, and filtering
- **Console & logs** — real-time output from the build process and daemon logs
- **Multiple keymaps** — Emacs, Vim, and VSCode keybindings (toggle with `Ctrl+K`)
- **Search** — fuzzy search across projects and network events

```bash
# Start the terminal UI
ewd-tui

# Or from the workspace
cargo run --release -p ewd-tui
```

See [ewd-tui/README.md](ewd-tui/README.md) for full documentation.

## Emacs Integration

[`ewd.el`](emacs/README.md) is an Emacs Lisp client that connects to the daemon via Unix socket, providing:

- Tab management from within Emacs (`*ewd-tabs*` buffer)
- JavaScript evaluation for web scraping and automation
- Show/hide tabs interactively (CAPTCHA solving)
- Push-based updates via `tabs_changed` notifications
- Owner-based access control

**Global keybindings** (`C-c w` prefix):

| Keybinding | Command             | Description                               |
|------------|---------------------|-------------------------------------------|
| `C-c w c`  | `ewd-connect`       | Connect to the daemon                     |
| `C-c w t`  | `ewd-create-tab`    | Create a new tab                          |
| `C-c w l`  | `ewd-list-tabs`     | Show `*ewd-tabs*` buffer                  |
| `C-c w k`  | `ewd-close-tab`     | Close a tab by ID                         |
| `C-c w e`  | `ewd-eval`          | Evaluate JavaScript in a tab              |
| `C-c w n`  | `ewd-navigate`      | Navigate a tab to a new URL               |
| `C-c w r`  | `ewd-reload`        | Reload a tab                              |
| `C-c w S`  | `ewd-show-tab`      | Show tab in GTK window                    |
| `C-c w H`  | `ewd-hide-tab`      | Hide tab (convert to headless)            |

Additional Emacs Lisp packages are available in [`elisp-packages/`](elisp-packages/):
- [`ewd-deepseek`](elisp-packages/ewd-deepseek/) — DeepSeek AI integration
- [`ewd-translate`](elisp-packages/ewd-translate/) — Translation plugin

See [emacs/README.md](emacs/README.md) for full documentation.

## Testing

### Unit Tests

```bash
cargo test
```

### Integration Tests

```bash
./bash/integration.sh
```

See [bash/README.md](bash/README.md) for details.

### Test Suites

| Suite         | Description                                      |
|---------------|--------------------------------------------------|
| `basic`       | Tab lifecycle (create, list, close)              |
| `eval`        | JavaScript evaluation                            |
| `errors`      | Error handling (non-existent tab, invalid UUID)  |
| `concurrent`  | 10 concurrent tabs create/close                  |
| `orphan`      | Orphan cleanup (~65s, requires 60s interval)     |
| `shutdown`    | Graceful daemon shutdown                         |

## Documentation

- [Protocol Specification](docs/PROTOCOL.md) — JSON-RPC protocol details
- [Events Reference](docs/EVENTS.md) — Event system documentation

## Troubleshooting

### Socket Permission Denied

Ensure your user has read/write access to the socket:

```bash
ls -l $XDG_RUNTIME_DIR/webkit-bridge.sock
```

The daemon creates the socket with your user's permissions. If running as a different user, adjust permissions or use the same user.

### WebKitGTK Crashes on Wayland

Set the environment variables mentioned in the "Environment Variables" section. These disable problematic GPU acceleration features.

### Daemon Won't Start — Socket Already Exists

If the daemon crashed previously, the socket file may still exist. Remove it manually:

```bash
rm $XDG_RUNTIME_DIR/webkit-bridge.sock
```

Or the daemon will automatically clean it up on startup.

### Tabs Not Closing After Client Disconnects

Tabs are owned by the client process (identified by PID). If the process dies unexpectedly, the daemon's background cleanup task (runs every 60 seconds) will detect the dead PID and close orphaned tabs automatically.

### ewd-tui: Daemon Not Found

Ensure `ewd-daemon` is built and in PATH, or set `EWD_DAEMON_PATH`:

```bash
export EWD_DAEMON_PATH=/path/to/ewd-daemon
ewd-tui
```

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

1. Fork the repository
2. Create a feature branch
3. Test your changes with `cargo test` and `./bash/integration.sh`
4. Submit a pull request
