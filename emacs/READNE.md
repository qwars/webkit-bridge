# EWD — Emacs WebKit Daemon Client

An Emacs client for the [webkit-bridge](https://github.com/your-repo/webkit-bridge) daemon, providing seamless browser automation and web browsing directly from Emacs via WebKitGTK.

## 🎯 Overview

**EWD (Emacs WebKit Daemon)** is an Emacs Lisp package that connects to the `webkit-bridge` daemon through a Unix socket using the JSON-RPC 2.0 protocol. It allows you to:

- Browse the web using WebKitGTK without leaving Emacs
- Automate web interactions via JavaScript evaluation
- Manage multiple browser tabs with a dedicated buffer interface
- Scrape web content programmatically
- Integrate web browsing with your Emacs workflow

EWD is **not** a browser itself — it's a client that talks to the standalone `webkit-bridge` daemon, which manages the actual WebKitGTK instances.

## ✨ Features

### Tab Management
- Create, close, navigate, and reload tabs
- Dedicated `*ewd-tabs*` buffer for visual tab management
- Automatic tab refresh on events (title changes, tab creation/closure)
- Visual distinction between tabs owned by Emacs (`*`) and external tabs

### JavaScript Evaluation
- Execute arbitrary JavaScript in any tab context
- Get results back into Emacs for further processing
- Perfect for web scraping and automation

### Security & Isolation
- **Owner-based access control**: Emacs can only manage tabs it created
- Tabs created by other clients (e.g., `wkctl`, other Emacs instances) are visible but not modifiable
- Automatic cleanup: all Emacs-owned tabs are closed when Emacs exits

### Reactive UI
- Real-time updates when tab titles change
- Automatic buffer refresh on tab creation/closure events
- Read-only buffer protection with safe programmatic updates

### Seamless Integration
- Auto-connect on first command (configurable)
- Silent cleanup on Emacs exit (no confirmation prompts)
- UTF-8 support for international content

## 📋 Requirements

### Emacs
- **Emacs 30.1+** (tested)
- Should work with Emacs 27.1+ (not tested)
- Compiled with `json-parse-string` support (standard in modern builds)

### Daemon
- **webkit-bridge daemon** must be running
- Socket available at `$XDG_RUNTIME_DIR/webkit-bridge.sock` (default)
- See [webkit-bridge README](../README.md) for daemon installation

### System
- Unix-like OS with Unix domain socket support
- Linux, BSD, macOS (with appropriate socket paths)

## 🚀 Installation

### Manual Installation

1. Clone or copy `ewd.el` to your Emacs load path:
   ```bash
   mkdir -p ~/.emacs.d/lisp
   cp ewd.el ~/.emacs.d/lisp/
   ```

2. Add to your `init.el`:
   ```elisp
   (add-to-list 'load-path "~/.emacs.d/lisp")
   (require 'ewd)
   ```

### Using use-package

```elisp
(use-package ewd
  :load-path "~/.emacs.d/lisp/ewd"
  :bind-keymap ("C-c w" . ewd-command-map)
  :config
  (setq ewd-auto-connect t))
```

## ⚙️ Configuration

### Customization Variables

Access via `M-x customize-group RET ewd RET`:

| Variable | Default | Description |
|----------|---------|-------------|
| `ewd-socket-path` | `nil` (auto-detect) | Path to daemon socket. If `nil`, uses `$XDG_RUNTIME_DIR/webkit-bridge.sock` or `/tmp/webkit-bridge.sock` |
| `ewd-default-visible` | `nil` | Default visibility for new tabs. If `t`, tabs open in GTK windows |
| `ewd-auto-connect` | `t` | Automatically connect to daemon on first command |

### Example Configuration

```elisp
(use-package ewd
  :load-path "~/.emacs.d/lisp/ewd"
  :custom
  (ewd-socket-path "/run/user/1000/webkit-bridge.sock")
  (ewd-default-visible t)
  (ewd-auto-connect t)
  :bind-keymap ("C-c w" . ewd-command-map))
```

## ⌨️ Usage

### Keybindings

All commands are bound under the `C-c w` prefix:

| Keybinding | Command | Description |
|------------|---------|-------------|
| `C-c w C` | `ewd-connect` | Connect to the daemon |
| `C-c w D` | `ewd-disconnect` | Disconnect from the daemon |
| `C-c w c` | `ewd-create-tab` | Create a new tab (prompts for URL) |
| `C-c w k` | `ewd-close-tab` | Close a tab by ID |
| `C-c w l` | `ewd-list-tabs` | Show list of all tabs in dedicated buffer |
| `C-c w n` | `ewd-navigate` | Navigate a tab to a new URL |
| `C-c w r` | `ewd-reload` | Reload a tab |
| `C-c w e` | `ewd-eval` | Evaluate JavaScript in a tab |
| `C-c w s` | `ewd-shutdown` | Shutdown the daemon |

### The `*ewd-tabs*` Buffer

Press `C-c w l` to open the tabs buffer. It displays all open tabs with the following format:

```
EWD Tabs (2 total)
------------------------------------------------------------

* ac93f48c-9b3b-4b64-8852-966d2af207b9  Example Domain
  URL: https://example.com [owner: eff7e05a]

  bb91aed9-1d07-46ef-87ae-f4d75689881c  Google
  URL: https://google.com [owner: 00000000]
```

**Legend:**
- `*` prefix = tab owned by this Emacs instance (can be modified)
- No prefix = tab owned by another client (read-only)
- `[owner: ...]` = short UUID of the owner client

**Buffer keybindings:**

| Key | Command | Description |
|-----|---------|-------------|
| `g` | `ewd-list-tabs` | Refresh the list |
| `q` | `quit-window` | Close the buffer |
| `d` | `ewd-close-tab-at-point` | Close the tab at point |
| `e` | `ewd-eval-tab-at-point` | Evaluate JS in the tab at point |

### Typical Workflow

1. **Start the daemon** (if not running as a service):
   ```bash
   ewd-daemon
   ```

2. **Connect from Emacs**:
   - Press `C-c w C` (or just run any command — auto-connect is enabled by default)

3. **Create a tab**:
   - Press `C-c w c`
   - Enter URL: `https://example.com`
   - Press `RET`

4. **View tabs**:
   - Press `C-c w l`
   - Navigate with arrow keys
   - Press `d` to close, `e` to evaluate JS

5. **Evaluate JavaScript**:
   - Press `C-c w e`
   - Enter tab ID (or use `e` in the tabs buffer)
   - Enter script: `document.title`
   - Result appears in the minibuffer

6. **Exit Emacs**:
   - All tabs owned by this Emacs instance are automatically closed
   - No confirmation prompts
   - Other clients' tabs remain untouched

## 📖 Examples

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

(defun my-close-all-my-tabs ()
  "Close all tabs owned by this Emacs instance."
  (interactive)
  (dolist (tab-id ewd--my-tabs)
    (ewd-close-tab tab-id)))
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

## 🔧 Troubleshooting

### "Socket file does not exist"

The daemon is not running. Start it:
```bash
ewd-daemon
```

Or check if it's running as a systemd service:
```bash
systemctl --user status ewd-daemon
```

### "Not connected to EWD daemon"

Run `M-x ewd-connect` or ensure `ewd-auto-connect` is set to `t`.

### "Cannot close tab ... Not created by this instance"

You're trying to close a tab owned by another client (e.g., `wkctl` or another Emacs instance). Only the owner can modify a tab. Use `C-c w l` to see which tabs you own (marked with `*`).

### Buffer doesn't auto-refresh

The buffer should refresh automatically on events. If it doesn't:
1. Press `g` in the `*ewd-tabs*` buffer to manually refresh
2. Check that the daemon is sending events (see daemon logs)
3. Ensure you're using Emacs 30+ for best compatibility

### "Symbol's value as variable is void" errors

You're using an old version of `ewd.el`. Update to v0.6.0+ which is compatible with Emacs 30+ lexical binding.

## 🏗️ Architecture

```
┌─────────────┐         Unix Socket          ┌──────────────────┐
│   Emacs     │  ◄──── JSON-RPC 2.0 ────►    │ webkit-bridge    │
│  (ewd.el)   │        (NDJSON format)       │    daemon        │
└─────────────┘                              └──────────────────┘
                                                      │
                                                      ▼
                                              ┌──────────────┐
                                              │  WebKitGTK   │
                                              │   (tabs)     │
                                              └──────────────┘
```

- **Emacs client** (`ewd.el`): Sends JSON-RPC requests, receives responses and notifications
- **Daemon** (`webkit-bridge`): Manages WebKitGTK instances, enforces ownership rules
- **Tabs**: Actual browser instances, owned by specific clients

## 📝 License

[Add your license here]

## 🤝 Contributing

Contributions are welcome! Please:
1. Fork the repository
2. Create a feature branch
3. Test with Emacs 30+
4. Submit a pull request

## 🔗 Related Projects

- [webkit-bridge](../README.md) — The daemon that powers EWD
- [wkctl](../wkctl/README.md) — CLI utility for webkit-bridge

## 📚 See Also

- [JSON-RPC 2.0 Specification](https://www.jsonrpc.org/specification)
- [WebKitGTK Documentation](https://webkitgtk.org/)
- [Emacs Lisp Reference](https://www.gnu.org/software/emacs/manual/html_node/elisp/)
