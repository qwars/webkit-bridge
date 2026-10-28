# webkit-bridge

A high-performance, lightweight WebKitGTK daemon for headless and visible browser automation, controlled via JSON-RPC 2.0 over Unix sockets.

## Overview

**webkit-bridge** is a Rust-based project that provides a daemon (`ewd-daemon`) and CLI utility (`wkctl`) for managing WebKitGTK browser instances through a clean JSON-RPC API. It is designed for integration with Emacs, scripts, and other automation tools.

### Key Features

- **Isolated tab management** with automatic cleanup of orphaned tabs
- **Client-based access control** — each client can only manage tabs it created
- **Proxy support** (SOCKS5, HTTP) for anonymous browsing
- **Headless and visible modes** for flexible automation
- **Real-time event notifications** via JSON-RPC notifications
- **Zero-copy Unix socket IPC** for maximum performance

### Architecture

```
webkit-bridge/
├── ewd-core/          # Library crate (reusable API)
├── ewd-daemon/        # Binary crate (standalone daemon)
├── wkctl/             # CLI utility for testing
└── emacs/             # Emacs client (separate project)
```

## System Requirements

### Supported Platforms
- **OS**: Linux (tested on Debian 13, should work on any modern Linux distribution)
- **Display**: Wayland or X11
- **GPU**: Any GPU with proper drivers (tested on AMDGPU, Intel, NVIDIA)

### Required Dependencies

**System packages** (Debian/Ubuntu example):
```bash
sudo apt install \
  libwebkitgtk-6.0-dev \
  libgtk-4-dev \
  libsoup-3.0-dev \
  libjavascriptcoregtk-6.0-dev \
  pkg-config \
  build-essential
```

**Rust toolchain**:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Environment Variables

**Recommended for Wayland/AMDGPU stability** (optional but recommended):
```bash
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export GDK_BACKEND=wayland
```

Add these to your `~/.profile`, `~/.bashrc`, or systemd service file.

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

### Install System-wide (Optional)

```bash
sudo cp target/release/ewd-daemon /usr/local/bin/
sudo cp target/release/wkctl /usr/local/bin/
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
2. Create Unix socket at `$XDG_RUNTIME_DIR/webkit-bridge.sock` (or `/tmp/webkit-bridge.sock`)
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

## JSON-RPC API Reference

The daemon communicates via **JSON-RPC 2.0** over Unix domain sockets using **Newline-Delimited JSON (NDJSON)** format.

**Socket path**: `$XDG_RUNTIME_DIR/webkit-bridge.sock`

### Client Identification

Each connection receives a unique `client_id` (UUID). Clients can override this by including `"client_id"` in request params (used by `wkctl` with fixed ID `00000000-0000-0000-0000-000000000000`).

### Methods

#### `tab.create`

Create a new browser tab.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.create",
  "params": {
    "url": "https://example.com",
    "visible": true,
    "profile": "default"
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

**Parameters:**
- `url` (string, optional): URL to load. Default: `"about:blank"`
- `visible` (boolean, optional): Show GTK window. Default: `false`
- `profile` (string, optional): Profile name. Default: `"default"`

---

#### `tab.close`

Close an existing tab. **Only the owner client can close its tabs.**

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.close",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9"
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

---

#### `tab.navigate`

Navigate a tab to a new URL. **Only the owner client can navigate its tabs.**

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.navigate",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "url": "https://example.org"
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

---

#### `tab.reload`

Reload a tab. **Only the owner client can reload its tabs.**

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.reload",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9"
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

---

#### `tab.eval`

Execute JavaScript in a tab context. **Only the owner client can evaluate JS in its tabs.**

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.eval",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "script": "document.title"
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

---

#### `tab.list`

List all open tabs. **Available to all clients (no ownership check).**

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.list",
  "params": {},
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
      "owner": "00000000"
    },
    {
      "id": "bb91aed9-1d07-46ef-87ae-f4d75689881c",
      "url": "https://google.com",
      "title": "Google",
      "owner": "eff7e05a"
    }
  ],
  "id": 6
}
```

**Fields:**
- `id` (string): Tab UUID
- `url` (string): Current URL
- `title` (string): Page title
- `owner` (string): Short UUID (first 8 chars) of the owner client

---

#### `daemon.shutdown`

Shutdown the daemon gracefully.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "daemon.shutdown",
  "params": {},
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

### Error Codes

- `-32600`: Invalid Request
- `-32601`: Method not found
- `-32602`: Invalid params
- `-32000`: Generic error (tab not found, not owner, eval error, etc.)

## Testing with wkctl

The `wkctl` CLI utility provides a convenient way to test the daemon:

```bash
# Create a visible tab
wkctl create --visible https://example.com

# List all tabs
wkctl list

# Navigate to a new URL
wkctl navigate <tab-id> https://example.org

# Reload the tab
wkctl reload <tab-id>

# Evaluate JavaScript
wkctl eval <tab-id> "document.title"

# Close the tab
wkctl close <tab-id>
```

**Note:** `wkctl` uses a fixed `client_id` (`00000000-0000-0000-0000-000000000000`), so all `wkctl` invocations share ownership of tabs created via CLI.

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

## License

[Add your license here]

## Contributing
