# EWD — Emacs WebKit Daemon Client

An Emacs client for the [webkit-bridge](https://github.com/your-repo/webkit-bridge) daemon, providing seamless browser automation and web browsing directly from Emacs via WebKitGTK.

## Overview

EWD (Emacs WebKit Daemon) is an Emacs Lisp package that connects to the `webkit-bridge` daemon through a Unix socket using the JSON-RPC 2.0 protocol. It allows you to:

- Browse the web using WebKitGTK without leaving Emacs
- Automate web interactions via JavaScript evaluation
- Manage multiple browser tabs with a dedicated buffer interface
- Scrape web content programmatically
- Integrate web browsing with your Emacs workflow

EWD is not a browser itself — it's a client that talks to the standalone `webkit-bridge` daemon, which manages the actual WebKitGTK instances.

## Features

### Tab Management
- Create, close, navigate, and reload tabs
- Dedicated `*ewd-tabs*` buffer for visual tab management
- **Push-based updates**: the buffer refreshes automatically when any tab state changes (via `tabs_changed` notification)
- **Cursor preservation**: cursor position is preserved across buffer redraws
- Visual distinction: `*` marker for tabs owned by Emacs, gray text for external tabs

### JavaScript Evaluation
- Execute arbitrary JavaScript in any tab context
- Get results back into Emacs for further processing
- Perfect for web scraping and automation

### Security & Isolation
- **Owner-based access control**: Emacs can only manage tabs it created
- Tabs created by other clients (e.g., `wkctl`, other Emacs instances) are visible but displayed in gray
- **Automatic cleanup**: all Emacs-owned tabs are closed silently when Emacs exits (no confirmation prompts)

### Reactive UI
- Real-time updates via `tabs_changed` notifications from the daemon
- Read-only buffer protection with safe programmatic updates
- Commands work from any line within a tab's block (UUID, URL, or Owner line)

## Requirements

### Emacs
- **Emacs 30+** (required)
- Compiled with `json-parse-string` support (standard in modern builds)

### Daemon
- `webkit-bridge` daemon must be running
- Socket available at `$XDG_RUNTIME_DIR/webkit-bridge.sock` (default)
- Override via `EWD_SOCKET_PATH` environment variable or `ewd-socket-path` customization
- See [webkit-bridge README](../README.md) for daemon installation

### System
- Unix-like OS with Unix domain socket support
- Linux, BSD, macOS (with appropriate socket paths)

## Installation

### Manual Installation

Clone or copy `ewd.el` to your Emacs load path:

```bash
mkdir -p ~/.emacs.d/lisp
cp ewd.el ~/.emacs.d/lisp/
```

Add to your `init.el`:

```elisp
(add-to-list 'load-path "~/.emacs.d/lisp")
(require 'ewd)
```

### Using use-package

```elisp
(use-package ewd
  :load-path "~/.emacs.d/lisp/ewd"
  :config
  (setq ewd-socket-path "/run/user/1000/webkit-bridge.sock"))
```

## Configuration

### Customization Variables

Access via `M-x customize-group RET ewd RET`:

`ewd-socket-path` `nil`
	(auto-detect) Path to daemon socket. If `nil`, uses `$EWD_SOCKET_PATH`, then `$XDG_RUNTIME_DIR/webkit-bridge.sock`, then `/tmp/webkit-bridge.sock`

### Example Configuration

```elisp
(use-package ewd
  :load-path "~/.emacs.d/lisp/ewd"
  :custom
  (ewd-socket-path "/run/user/1000/webkit-bridge.sock"))
```

## Usage

### Global Keybindings

All commands are bound under the `C-c w` prefix:

| Keybinding | Command             | Description                               |
|------------|---------------------|-------------------------------------------|
| `C-c w c`  | `ewd-connect`       | Connect to the daemon                     |
| `C-c w d`  | `ewd-disconnect`    | Disconnect from the daemon                |
| `C-c w t`  | `ewd-create-tab`    | Create a new tab (prompts for URL)        |
| `C-c w l`  | `ewd-list-tabs`     | Show list of all tabs in dedicated buffer |
| `C-c w k`  | `ewd-close-tab`     | Close a tab by ID                         |
| `C-c w e`  | `ewd-eval`          | Evaluate JavaScript in a tab              |
| `C-c w n`  | `ewd-navigate`      | Navigate a tab to a new URL               |
| `C-c w r`  | `ewd-reload`        | Reload a tab                              |
| `C-c w o`  | `ewd-switch-to-tab` | Open tab URL in system browser            |

### The `*ewd-tabs*` Buffer

Press `C-c w l` to open the tabs buffer. It displays all open tabs with the following format:

```
EWD Tabs (2 open, client 1e04045b-5b84-4b71-b11f-7cb85e5f4846)
============================================================

Legend: * = owned by this Emacs session
        gray text = owned by another client

* [ed93a3ff-51bf-4abb-b23a-fba027dfca9f] Example Domain
  URL: https://example.com
  Owner: 1e04045b

  [88341d6e-ddea-4b14-9278-e95f538c3b57] Google
  URL: https://google.com
  Owner: 00000000
```

**Legend:**
- `*` prefix = tab owned by this Emacs instance (can be modified)
- No prefix + gray text = tab owned by another client (read-only in this session)
- `Owner:` = short UUID (8 chars) of the owner client

### Buffer Keybindings

| Key   | Command                  | Description             |
|-------|--------------------------|-------------------------|
| `g`   | `ewd-list-tabs`          | Refresh the list        |
| `k`   | `ewd-close-tab-at-point` | Close the tab at point  |
| `o`   | `ewd-open-tab-at-point`  | Open tab URL in browser |
| `RET` | `ewd-open-tab-at-point`  | Open tab URL in browser |
| `q`   | `quit-window`            | Close the buffer        |

**Note:** `k`, `o`, and `RET` work from **any line** within the tab's block (UUID line, URL line, or Owner line).

### Typical Workflow

1. **Start the daemon** (if not running as a service):
   ```bash
   ewd-daemon
   ```

2. **Connect from Emacs:**
   Press `C-c w c`

3. **Create a tab:**
   - Press `C-c w t`
   - Enter URL: `https://example.com`
   - Press `RET`

4. **View tabs:**
   - Press `C-c w l`
   - Navigate with arrow keys
   - Press `k` to close, `o` or `RET` to open in browser

5. **Evaluate JavaScript:**
   - Press `C-c w e`
   - Enter tab ID (or use `e` in the tabs buffer)
   - Enter script: `document.title`
   - Result appears in the minibuffer and `*ewd-eval*` buffer

6. **Exit Emacs:**
   - All tabs owned by this Emacs instance are automatically closed
   - No confirmation prompts
   - Other clients' tabs remain untouched

## Examples

### Web Scraping

```elisp
;; Fetch page title
(ewd-eval "tab-id" "document.title")

;; Extract all links
(ewd-eval "tab-id"
  "Array.from(document.querySelectorAll('a')).map(a => ({href: a.href, text: a.textContent}))")

;; Get page content as text
(ewd-eval "tab-id" "document.body.innerText")
```

### Automation Script

```elisp
(defun my-open-research-tabs ()
  "Open a set of research tabs."
  (interactive)
  (dolist (url '("https://google.com"
                 "https://arxiv.org"
                 "https://scholar.google.com"))
    (ewd-create-tab url)))
```

### Integration with org-mode

```elisp
(defun my-org-browse-url-in-ewd (url)
  "Open URL in EWD instead of default browser."
  (interactive "sURL: ")
  (ewd-create-tab url))

;; Use as org-mode link handler
(setq browse-url-browser-function #'my-org-browse-url-in-ewd)
```

## Troubleshooting

### "EWD: Not connected. Run M-x ewd-connect first"

The daemon is not running or you're not connected. Start it:

```bash
ewd-daemon
```

Then run `M-x ewd-connect` in Emacs.

### "Socket file does not exist"

Check the socket path:

```bash
ls -l $XDG_RUNTIME_DIR/webkit-bridge.sock
# or
ls -l /tmp/webkit-bridge.sock
```

If the daemon uses a custom path, set `ewd-socket-path`:

```elisp
(setq ewd-socket-path "/custom/path/webkit-bridge.sock")
```

### Buffer doesn't auto-refresh

The buffer refreshes automatically when the daemon sends `tabs_changed` notifications. If it doesn't:

- Press `g` in the `*ewd-tabs*` buffer to manually refresh
- Check that the daemon is running and connected
- Check daemon logs for any errors

### "EWD: Error parsing message"

You may be using an incompatible version of the daemon. Ensure you're using the latest `webkit-bridge` daemon that supports the `tabs_changed` notification protocol.

## Architecture

```
┌─────────────┐         Unix Socket          ┌──────────────────┐
│   Emacs     │  ◄──── JSON-RPC 2.0 ────►    │ webkit-bridge    │
│  (ewd.el)   │        (NDJSON format)       │    daemon        │
└─────────────┘                              └──────────────────┘
       │                                              │
       │                                      ┌───────┴────────┐
       │                                      │  WebKitGTK     │
       │                                      │   (tabs)       │
       │                                      └────────────────┘
       │
       ▼
  tabs_changed notification
  (push-based updates)
```

- **Emacs client (`ewd.el`)**: Sends JSON-RPC requests, receives responses and `tabs_changed` notifications
- **Daemon (`webkit-bridge`)**: Manages WebKitGTK instances, enforces ownership rules, broadcasts state changes
- **Tabs**: Actual browser instances, owned by specific clients

## License


## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Test with Emacs 30+
4. Submit a pull request

## Related Projects

- [webkit-bridge](../README.md) — The daemon that powers EWD
- [wkctl](../wkctl/README.md) — CLI utility for webkit-bridge

## See Also

- [JSON-RPC 2.0 Specification](https://www.jsonrpc.org/specification)
- [WebKitGTK Documentation](https://webkitgtk.org/)
- [Emacs Lisp Reference](https://www.gnu.org/software/emacs/manual/html_node/elisp/)
