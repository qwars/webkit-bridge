# EWD — Emacs WebKit Daemon Client

An Emacs client for the [webkit-bridge](../../README.md) daemon, providing browser automation, tab management, and web browsing directly from Emacs via WebKitGTK.

**Package version:** 0.9.0  
**Requires:** Emacs 28.1+ (30+ recommended), with `json-parse-string` support

## Overview

EWD (Emacs WebKit Daemon) is an Emacs Lisp package that connects to the `ewd-daemon` through a Unix socket using the JSON-RPC 2.0 protocol. It provides a **dashboard UI** for visual tab management, a **programmatic API** for scripting and automation, a **JS bridge** for custom JavaScript message handling, and a **debug log** with Org-mode based per-tab logging.

**EWD is not a browser itself** — it's a client that talks to the standalone `ewd-daemon`, which manages the actual WebKitGTK instances.

### Components

The package consists of three files:

| File               | Description                                                                    |
|--------------------|--------------------------------------------------------------------------------|
| `ewd.el`           | Core client — connection, JSON-RPC protocol, async/sync API, JS handler system |
| `ewd-dashboard.el` | Dashboard UI — tabular tab listing in `*ewd*` buffer with interactive commands |
| `ewd-debug.el`     | Debug logging — Org-mode based per-tab log in `*ewd-debug*` buffer             |

## Features

### Tab Management
- Create, close, navigate, and reload tabs
- **Show/hide tabs**: toggle GTK window visibility for any tab (useful for CAPTCHAs and interactive pages)
- Dedicated `*ewd*` dashboard buffer with tabular display
- Push-based updates: the buffer refreshes automatically via `tabs_changed` notifications
- Visual distinction: visible tabs are highlighted; tab ID, URL, title, and visibility status are shown in columns

### JavaScript Evaluation
- Execute arbitrary JavaScript in any tab context
- Get results back into Emacs for further processing
- Ideal for web scraping and automation

### JS Bridge (Custom Handlers)
- Register custom JavaScript handlers via `ewd-register-js-handler`
- Receive messages from JavaScript code running in the browser
- Useful for building interactive Emacs-browser integrations

### Security & Isolation
- Owner-based access control: Emacs can only manage tabs it created
- Tabs created by other clients (e.g., `wkctl`, other Emacs instances) are visible but cannot be modified
- Automatic cleanup: all Emacs-owned tabs are closed silently when Emacs exits (no confirmation prompts)

### Dashboard UI
- Tabular layout: columns for URL, visibility, title, and ID
- Keyboard-driven: close, show, hide, eval, browse tabs without leaving Emacs
- Auto-refresh via push notifications from the daemon
- Empty state and not-connected state guidance

### Debug Logging
- Org-mode based `*ewd-debug*` buffer with per-tab sections
- Batched, deferred log insertion (flushes when Emacs is idle)
- Preserves fold state, cursor position, and scroll position across flushes
- Dynamic heading updates (title, URL changes reflected automatically)

## Requirements

### Emacs
- Emacs 28.1+ (30+ recommended)
- Compiled with `json-parse-string` support

### Daemon
- `ewd-daemon` must be built and available in `PATH`
- Socket available at `$XDG_RUNTIME_DIR/webkit-bridge.sock` (default)
- Override via `EWD_SOCKET_PATH` environment variable or `ewd-socket-path` customization
- The daemon can be auto-started by EWD if `ewd-daemon` is found in `PATH`
- See [webkit-bridge README](../../README.md) for daemon installation

### System
- Unix-like OS with Unix domain socket support
- Linux, BSD, macOS (with appropriate socket paths)

## Installation

### Manual Installation

Clone or copy the `ewd/` directory to your Emacs load path:

```bash
mkdir -p ~/.emacs.d/lisp
cp -r ewd/ ~/.emacs.d/lisp/
```

Add to your `init.el`:

```elisp
(add-to-list 'load-path "~/.emacs.d/lisp/ewd")
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

| Variable                    | Default             | Description                                                                                          |
|-----------------------------|---------------------|------------------------------------------------------------------------------------------------------|
| `ewd-socket-path`           | `nil` (auto-detect) | Path to daemon socket. If `nil`, uses `$EWD_SOCKET_PATH`, then `$XDG_RUNTIME_DIR/webkit-bridge.sock` |
| `ewd-dashboard-enabled`     | `t`                 | Enable dashboard UI                                                                                  |
| `ewd-dashboard-url-width`   | `45`                | Width of URL column in dashboard                                                                     |
| `ewd-dashboard-title-width` | `35`                | Width of TITLE column in dashboard                                                                   |
| `ewd-dashboard-id-width`    | `8`                 | Width of ID column (first N chars of UUID)                                                           |

### Example Configuration

```elisp
(use-package ewd
  :load-path "~/.emacs.d/lisp/ewd"
  :custom
  (ewd-socket-path "/run/user/1000/webkit-bridge.sock")
  (ewd-dashboard-url-width 50)
  (ewd-dashboard-title-width 40))
```

## Usage

### Connecting to the Daemon

```elisp
M-x ewd-connect
```

EWD will first look for a running `ewd-daemon` process. If none is found and the daemon binary is in `PATH`, it will auto-start it. Then it connects via Unix socket.

To disconnect:

```elisp
M-x ewd-disconnect
```

### Global Keybindings

All commands are bound under the `C-c w` prefix:

| Keybinding | Command                           | Description                                   |
|------------|-----------------------------------|-----------------------------------------------|
| `C-c w c`  | `ewd-connect`                     | Connect to the daemon (auto-starts if needed) |
| `C-c w d`  | `ewd-disconnect`                  | Disconnect from the daemon                    |
| `C-c w t`  | `ewd-create-tab`                  | Create a new tab (prompts for URL)            |
| `C-c w l`  | `ewd-list-tabs` / `ewd-dashboard` | Show the `*ewd*` dashboard                    |
| `C-c w k`  | `ewd-close-tab`                   | Close a tab by ID                             |
| `C-c w e`  | `ewd-eval`                        | Evaluate JavaScript in a tab                  |
| `C-c w n`  | `ewd-navigate`                    | Navigate a tab to a new URL                   |
| `C-c w r`  | `ewd-reload`                      | Reload a tab                                  |
| `C-c w o`  | `ewd-switch-to-tab`               | Open tab URL in system browser                |
| `C-c w S`  | `ewd-show-tab`                    | Show tab in GTK window                        |
| `C-c w H`  | `ewd-hide-tab`                    | Hide tab (convert to headless)                |

### The Dashboard (`*ewd*` Buffer)

Press `C-c w l` to open the dashboard. It displays all open tabs in a tabular format:

```
URL                          | VIS   | TITLE                            | ID
-----------------------------------------------------------------------------------
https://example.com          | yes   | Example Domain                   | ac93f48c
https://google.com           | no    | Google                           | bb91aed9
```

**Keybindings in the dashboard buffer:**

| Key         | Command                    | Description                      |
|-------------|----------------------------|----------------------------------|
| `g`         | `ewd-dashboard-refresh`    | Refresh the list                 |
| `k`         | `ewd-dashboard-close`      | Close tab at point               |
| `o` / `RET` | `ewd-dashboard-browse-url` | Open tab URL in external browser |
| `s`         | `ewd-dashboard-show`       | Show tab in GTK window           |
| `h`         | `ewd-dashboard-hide`       | Hide tab (make headless)         |
| `e`         | `ewd-dashboard-eval`       | Evaluate JS in tab at point      |
| `t`         | `ewd-create-tab`           | Create a new tab                 |
| `c`         | `ewd-dashboard-copy-id`    | Copy full tab ID to kill-ring    |
| `q`         | `quit-window`              | Close the dashboard window       |

### Debug Log (`*ewd-debug*` Buffer)

The debug log is created automatically when EWD starts logging. It uses Org-mode for structured, per-tab logging:

```
* EWD Debug Log [[ipc][ipc]]
#+BEGIN_EXAMPLE
[14:30:01] Connected to daemon
[14:30:02] Request: create {"url":"https://example.com"}
#+END_EXAMPLE

* Example Domain [[https://example.com][link]] [[ac93f48c-...][uuid]]
#+BEGIN_EXAMPLE
[14:30:03] Tab created: ac93f48c-9b3b-4b64-8852-966d2af207b9
[14:30:05] Navigate to https://example.com
#+END_EXAMPLE
```

Each tab gets its own section. Sections can be folded/unfolded using Org-mode keybindings. The buffer updates in real-time, preserving your cursor position and fold state.

### Typical Workflow

1. **Start the daemon** (if not running as a service):
   ```bash
   ewd-daemon
   ```

2. **Connect from Emacs:**
   `M-x ewd-connect` or press `C-c w c`

3. **Create a tab:**
   `M-x ewd-create-tab` or press `C-c w t`, enter URL, press `RET`

4. **View and manage tabs:**
   `M-x ewd-dashboard` or press `C-c w l`
   - Navigate with arrow keys
   - Press `k` to close, `o` or `RET` to open in browser
   - Press `s` to show in GTK window, `h` to hide

5. **Handle CAPTCHA or interactive page:**
   - Press `s` on the tab to show its GTK window
   - Solve the CAPTCHA manually
   - Press `h` to hide the window again

6. **Evaluate JavaScript:**
   `M-x ewd-eval` or press `C-c w e`, enter tab ID, enter script
   Result appears in the minibuffer

7. **Exit Emacs:**
   - All tabs owned by this Emacs instance are automatically closed
   - No confirmation prompts
   - Other clients' tabs remain untouched

## Programmatic API

### Synchronous Functions

These functions send a request and wait for the response:

```elisp
(ewd-create-tab url &optional visible)
(ewd-close-tab tab-id)
(ewd-navigate tab-id url)
(ewd-reload tab-id)
(ewd-show tab-id)
(ewd-hide tab-id)
(ewd-eval tab-id script)
(ewd-list-tabs)
(ewd-shutdown)
```

### Asynchronous Functions

These functions accept success and error callbacks:

```elisp
(ewd-create-tab-async url success-cb error-cb &optional visible)
(ewd-close-tab-async tab-id success-cb error-cb)
(ewd-navigate-async tab-id url success-cb error-cb)
(ewd-reload-async tab-id success-cb error-cb)
(ewd-show-async tab-id success-cb error-cb)
(ewd-hide-async tab-id success-cb error-cb)
(ewd-eval-async tab-id script success-cb error-cb)
(ewd-list-tabs-async success-cb error-cb)
(ewd-shutdown-async success-cb error-cb)
```

### JS Handler System

Register a handler to receive messages from JavaScript running in the browser:

```elisp
(ewd-register-js-handler "my-handler"
  (lambda (tab-id data)
    (message "EWD JS from %s: %s" tab-id data)))
```

JavaScript in the browser can then send messages via the WebKit user content manager.

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

### Async Tab Creation with Notification

```elisp
(ewd-create-tab-async
 "https://example.com"
 (lambda (msg)
   (let ((tab-id (ewd--extract-create-id msg)))
     (message "Tab created: %s" tab-id)))
 (lambda (err)
   (message "Failed to create tab: %s" err)))
```

## Troubleshooting

### "EWD: Not connected. Run M-x ewd-connect first"

The daemon is not running or you're not connected. Run `M-x ewd-connect`. EWD will attempt to auto-start the daemon if the binary is in `PATH`.

### "Socket file does not exist"

Check the socket path:

```bash
ls -l $XDG_RUNTIME_DIR/webkit-bridge.sock
```

If the daemon uses a custom path, set `ewd-socket-path`:

```elisp
(setq ewd-socket-path "/custom/path/webkit-bridge.sock")
```

### Dashboard doesn't auto-refresh

The dashboard refreshes automatically when the daemon sends `tabs_changed` notifications. If it doesn't:

- Press `g` in the `*ewd*` buffer to manually refresh
- Check that the daemon is running and connected
- Check daemon logs for any errors
- Verify `ewd-dashboard-enabled` is set to `t`

### "EWD: Error parsing message"

You may be using an incompatible version of the daemon. Ensure you're using the latest `ewd-daemon` that supports the `tabs_changed` notification protocol.

### Debug Logging

The debug buffer (`*ewd-debug*`) is created automatically. To view it:

```elisp
(switch-to-buffer "*ewd-debug*")
```

Use Org-mode keybindings to fold/unfold sections (`TAB` on a heading, `S-TAB` for global cycling).

## Architecture

```
┌─────────────┐         Unix Socket          ┌──────────────────┐
│   Emacs     │  ◄──── JSON-RPC 2.0 ────►    │  ewd-daemon      │
│  (ewd.el)   │        (NDJSON format)       │                  │
└──────┬──────┘                              └──────────────────┘
       │                                              │
  ┌────┴────┐                               ┌────────┴────────┐
  │ *ewd*   │                               │   WebKitGTK     │
  │dashboard│                               │   (tabs)        │
  └─────────┘                               └─────────────────┘
       │
  ┌────┴────┐
  │ *ewd-   │
  │ debug*  │
  └─────────┘
       │
  ┌────┴────┐
  │ JS      │
  │ handlers│
  └─────────┘
```

- **`ewd.el`** — Core: sends JSON-RPC requests, receives responses and `tabs_changed` notifications. Provides sync and async API, JS handler registration, and daemon lifecycle management.
- **`ewd-dashboard.el`** — Dashboard: renders the `*ewd*` buffer with a tabular view of all tabs. Provides keyboard-driven tab management.
- **`ewd-debug.el`** — Debug: logs all IPC traffic and tab events to the `*ewd-debug*` buffer using Org-mode sections, with batched/deferred insertion.
- **Daemon (`ewd-daemon`)** — Manages WebKitGTK instances, enforces ownership rules, broadcasts state changes via `tabs_changed` notifications.

## Related Projects

- [webkit-bridge](../../README.md) — The daemon that powers EWD
- [wkctl](../../wkctl/README.md) — CLI utility for webkit-bridge
- [ewd-tui](../../ewd-tui/README.md) — Terminal UI for webkit-bridge
- [ewd-deepseek](../ewd-deepseek/) — DeepSeek AI integration
- [ewd-translate](../ewd-translate/) — Translation plugin

## See Also

- [JSON-RPC 2.0 Specification](https://www.jsonrpc.org/specification)
- [WebKitGTK Documentation](https://webkitgtk.org/)
- [Emacs Lisp Reference](https://www.gnu.org/software/emacs/manual/html_node/elisp/)

## License

This project is licensed under the MIT License. See [LICENSE](../../LICENSE) for details.

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Test with Emacs 28.1+
4. Submit a pull request
