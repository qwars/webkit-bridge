;;; ewd.el --- Emacs WebKit Daemon client  -*- lexical-binding: t; -*-

;; Copyright (C) 2026
;; Author: EWD Project
;; Version: 0.8.2
;; Package-Requires: ((emacs "28.1"))
;; Keywords: comm, hypermedia, tools

;;; Commentary:
;; Emacs client for ewd-daemon — browser automation via JSON-RPC over Unix socket.
;; Provides dashboard UI in *ewd* buffer, programmatic API, and JS bridge handlers.

;;; Code:

(require 'json)
(require 'cl-lib)
(require 'browse-url)

;;; --- Customization ---

(defgroup ewd nil
  "Emacs WebKit Daemon client."
  :group 'comm
  :prefix "ewd-")

(defcustom ewd-daemon-path "ewd-daemon"
  "Path to the ewd-daemon binary."
  :type 'string
  :group 'ewd)

(defcustom ewd-daemon-directory nil
  "Directory containing ewd-daemon binary. Prepended to `exec-path'."
  :type '(choice (const nil) directory)
  :group 'ewd)

(defcustom ewd-daemon-profile "emacs"
  "Profile name for ewd-daemon."
  :type 'string
  :group 'ewd)

(defcustom ewd-daemon-proxy nil
  "Proxy URI for ewd-daemon (e.g., \"http://proxy:8080\")."
  :type '(choice (const nil) string)
  :group 'ewd)

(defcustom ewd-daemon-auto-start t
  "If non-nil, automatically start ewd-daemon if not running."
  :type 'boolean
  :group 'ewd)

(defcustom ewd-daemon-startup-timeout 5
  "Seconds to wait for daemon socket to appear."
  :type 'integer
  :group 'ewd)

(defcustom ewd-request-timeout 30
  "Timeout in seconds for JSON-RPC requests."
  :type 'integer
  :group 'ewd)

(defcustom ewd-debug nil
  "If non-nil, log all IPC messages to *ewd-debug* buffer."
  :type 'boolean
  :group 'ewd)

(defcustom ewd-dashboard-url-width 45
  "Width of URL column in dashboard."
  :type 'integer
  :group 'ewd)

(defcustom ewd-dashboard-title-width 35
  "Width of TITLE column in dashboard."
  :type 'integer
  :group 'ewd)

(defcustom ewd-dashboard-id-width 8
  "Width of ID column in dashboard (first N chars of UUID)."
  :type 'integer
  :group 'ewd)

;;; --- Faces ---

(defface ewd-dashboard-visible-face
  '((t :background "#4a7a4a" :foreground "white" :weight bold))
  "Face used for visible tabs in the EWD dashboard."
  :group 'ewd)

;;; --- Internal variables ---

(defvar ewd--process nil
  "Network process connected to ewd-daemon.")

(defvar ewd--socket-path nil
  "Path to the Unix domain socket for daemon connection.")

(defvar ewd--request-id 0
  "Current JSON-RPC request ID.")

(defvar ewd--pending-requests (make-hash-table :test 'eql)
  "Hash table of pending requests: ID -> (CALLBACK . TIMER).")

(defvar ewd--dashboard-buffer-name "*ewd*"
  "Name of the dashboard buffer.")

(defvar ewd--client-id nil
  "Client ID (UUID v4) assigned to this Emacs session.")

(defvar ewd--read-buffer ""
  "String buffer for accumulating incomplete NDJSON lines.")

(defvar ewd--dashboard-cache nil
  "Cached list of tabs for dashboard rendering.")

;;; --- Hooks ---

(defvar ewd-tab-created-hook nil
  "Hook run when a tab is created. Args: (TAB-ID URL).")

(defvar ewd-tab-closed-hook nil
  "Hook run when a tab is closed. Args: (TAB-ID).")

(defvar ewd-tab-navigated-hook nil
  "Hook run when a tab navigates. Args: (TAB-ID URL).")

(defvar ewd-tab-reloaded-hook nil
  "Hook run when a tab is reloaded. Args: (TAB-ID).")

(defvar ewd-tab-title-changed-hook nil
  "Hook run when a tab's title changes. Args: (TAB-ID TITLE).")

(defvar ewd-tab-visibility-changed-hook nil
  "Hook run when a tab's visibility changes. Args: (TAB-ID VISIBLE).")

(defvar ewd-network-event-hook nil
  "Hook run on network events. Args: (TAB-ID EVENT).")

(defvar ewd-js-message-hook nil
  "Hook run on JS bridge messages. Args: (TAB-ID MESSAGE-TYPE DATA).")

(defvar ewd-shutdown-hook nil
  "Hook run when daemon is shutting down.")

;;; --- JS Bridge handlers ---

(defvar ewd-js-handlers (make-hash-table :test 'equal)
  "Hash table of registered JS message handlers.
Keys are message-type strings, values are functions of two arguments:
\(fn TAB-ID DATA).")

(defun ewd-register-js-handler (message-type handler)
  "Register HANDLER for JS bridge messages of type MESSAGE-TYPE.
HANDLER is called with (TAB-ID DATA) when a matching message arrives.
Returns HANDLER."
  (puthash message-type handler ewd-js-handlers)
  (ewd--log "Registered JS handler for '%s'" message-type)
  handler)

(defun ewd-unregister-js-handler (message-type)
  "Unregister JS handler for MESSAGE-TYPE.
Returns non-nil if handler was registered."
  (prog1 (not (null (gethash message-type ewd-js-handlers)))
    (remhash message-type ewd-js-handlers)
    (ewd--log "Unregistered JS handler for '%s'" message-type)))

(defun ewd--js-message-dispatcher (tab-id message-type data)
  "Dispatch JS message to registered handler.
TAB-ID is the source tab, MESSAGE-TYPE is the message type string,
DATA is the message payload."
  (let ((handler (gethash message-type ewd-js-handlers)))
    (if handler
        (condition-case err
            (funcall handler tab-id data)
          (error
           (ewd--log "JS handler error for '%s': %s"
                     message-type
                     (error-message-string err))))
      (ewd--log "No handler registered for JS message type '%s'" message-type))))

(add-hook 'ewd-js-message-hook #'ewd--js-message-dispatcher)

;;; --- UUID generation ---

(defun ewd--generate-uuid ()
  "Generate a UUID v4 string (36 characters: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx)."
  (format "%08x-%04x-4%03x-%04x-%012x"
          (random (expt 16 8))
          (random (expt 16 4))
          (random (expt 16 3))
          (logior (random (expt 16 4)) #x8000)
          (random (expt 16 12))))

;;; --- Socket path ---

(defun ewd--generate-socket-path ()
  "Generate unique socket path for this Emacs instance."
  (let ((runtime-dir (or (getenv "XDG_RUNTIME_DIR")
                         (expand-file-name "~/.cache"))))
    (expand-file-name (format "ewd-emacs-%d.sock" (emacs-pid))
                      runtime-dir)))

;;; --- Logging ---

(defun ewd--log (format-string &rest args)
  "Log message if `ewd-debug' is non-nil."
  (when ewd-debug
    (with-current-buffer (get-buffer-create "*ewd-debug*")
      (goto-char (point-max))
      (insert (format "[%s] %s\n"
                      (format-time-string "%H:%M:%S")
                      (apply #'format format-string args))))))

;;; --- Daemon management ---

(defun ewd--find-daemon-binary ()
  "Find ewd-daemon binary in `exec-path'."
  (let ((exec-path (if ewd-daemon-directory
                        (cons ewd-daemon-directory exec-path)
                      exec-path)))
    (executable-find ewd-daemon-path)))

(defun ewd--start-daemon (socket-path)
  "Start ewd-daemon with SOCKET-PATH."
  (let ((daemon-binary (ewd--find-daemon-binary)))
    (unless daemon-binary
      (error "Cannot find ewd-daemon binary. Set `ewd-daemon-path' or `ewd-daemon-directory'"))
    (message "Starting ewd-daemon with profile '%s'..." ewd-daemon-profile)
    (let ((process-environment
           (cons (format "EWD_SOCKET_PATH=%s" socket-path)
                 process-environment)))
      (let ((args (list "--profile" ewd-daemon-profile)))
        (when ewd-daemon-proxy
          (setq args (append args (list "--proxy" ewd-daemon-proxy))))
        (let ((proc (apply #'start-process "ewd-daemon" nil daemon-binary args)))
          (set-process-query-on-exit-flag proc nil)
          (message "ewd-daemon started (PID: %d)" (process-id proc)))))
    ;; Wait for socket
    (let ((timeout ewd-daemon-startup-timeout)
          (elapsed 0))
      (while (and (not (file-exists-p socket-path))
                  (< elapsed timeout))
        (sleep-for 0.1)
        (setq elapsed (+ elapsed 0.1)))
      (unless (file-exists-p socket-path)
        (error "Timeout waiting for daemon socket after %d seconds" timeout)))))

(defun ewd--ensure-daemon ()
  "Ensure ewd-daemon is running and socket exists. Return socket path."
  (let ((socket-path (ewd--generate-socket-path)))
    (unless (file-exists-p socket-path)
      (if ewd-daemon-auto-start
          (ewd--start-daemon socket-path)
        (error "Daemon not running and auto-start disabled. Socket: %s" socket-path)))
    socket-path))

;;; --- Connection management ---

(defun ewd--connected-p ()
  "Return non-nil if connected to daemon."
  (and ewd--process (process-live-p ewd--process)))

;;;###autoload
(defun ewd-connect ()
  "Connect to ewd-daemon. Auto-starts if needed."
  (interactive)
  (when (ewd--connected-p)
    (user-error "Already connected to daemon"))
  (unless ewd--client-id
    (setq ewd--client-id (ewd--generate-uuid))
    (message "EWD: Generated client ID: %s" ewd--client-id))
  (let ((socket-path (ewd--ensure-daemon)))
    (setq ewd--socket-path socket-path
          ewd--read-buffer "")
    (setq ewd--process
          (make-network-process
           :name "ewd"
           :buffer nil
           :family 'local
           :service socket-path
           :filter #'ewd--process-filter
           :sentinel #'ewd--process-sentinel
           :nowait nil
           :coding 'utf-8-emacs-unix))
    (set-process-query-on-exit-flag ewd--process nil)
    (message "Connected to ewd-daemon at %s (client %s)" socket-path ewd--client-id)
    ;; (ewd-dashboard)
  ))

;;;###autoload
(defun ewd-disconnect ()
  "Disconnect from ewd-daemon."
  (interactive)
  (when ewd--process
    (delete-process ewd--process)
    (setq ewd--process nil
          ewd--socket-path nil))
  (maphash (lambda (_id data)
             (let ((timer (cdr data)))
               (when timer (cancel-timer timer))))
           ewd--pending-requests)
  (clrhash ewd--pending-requests)
  (setq ewd--dashboard-cache nil)
  (ewd-dashboard--refresh)
  (message "Disconnected from ewd-daemon"))

;;; --- Process filter/sentinel ---

(defun ewd--process-filter (_process output)
  "Process filter for ewd-daemon. Handles NDJSON OUTPUT."
  (setq ewd--read-buffer (concat ewd--read-buffer output))
  (ewd--log "<< %s" (string-trim output))
  (let ((pos 0)
        (line-end nil))
    (while (setq line-end (string-match "\n" ewd--read-buffer pos))
      (let ((line (substring ewd--read-buffer pos line-end)))
        (setq pos (1+ line-end))
        (unless (string-empty-p (string-trim line))
          (ewd--handle-message line))))
    (setq ewd--read-buffer (substring ewd--read-buffer pos))))

(defun ewd--process-sentinel (process event)
  "Process sentinel for ewd-daemon."
  (ewd--log "Process event: %s" (string-trim event))
  (when (not (process-live-p process))
    (setq ewd--process nil)
    (message "EWD: Connection %s" (string-trim event))
    (maphash (lambda (_id data)
               (let ((timer (cdr data)))
                 (when timer (cancel-timer timer))))
             ewd--pending-requests)
    (clrhash ewd--pending-requests)
    (setq ewd--dashboard-cache nil)
    (ewd-dashboard--refresh)))

;;; --- Message handling ---

(defun ewd--handle-message (line)
  "Handle a single JSON-RPC message LINE."
  (condition-case err
      (let* ((json-object-type 'alist)
             (json-array-type 'list)
             (json-key-type 'symbol)
             (json-false nil)
             (msg (json-read-from-string line)))
        (cond
         ((assq 'id msg)
          (ewd--handle-response msg))
         ((assq 'method msg)
          (ewd--handle-notification msg))
         (t
          (ewd--log "Unknown message format: %s" line))))
    (error
     (ewd--log "Error parsing message: %s - %s" line (error-message-string err)))))

(defun ewd--handle-response (msg)
  "Handle response MSG to a request."
  (let* ((id (cdr (assq 'id msg)))
         (id (if (stringp id) (string-to-number id) id))
         (pending (gethash id ewd--pending-requests)))
    (when pending
      (let ((callback (car pending))
            (timer (cdr pending)))
        (remhash id ewd--pending-requests)
        (when timer (cancel-timer timer))
        (when callback
          (funcall callback msg))))))

(defun ewd--handle-notification (msg)
  "Handle notification MSG from daemon."
  (let ((method (cdr (assq 'method msg)))
        (params (cdr (assq 'params msg))))
    (ewd--log "Event: %s %s" method params)
    (pcase (format "%s" method)
      ("tab_created"
       (run-hook-with-args 'ewd-tab-created-hook
                           (format "%s" (cdr (assq 'id params)))
                           (cdr (assq 'url params)))
       (ewd-dashboard--schedule-refresh))
      ("tab_closed"
       (run-hook-with-args 'ewd-tab-closed-hook
                           (format "%s" (cdr (assq 'id params))))
       (ewd-dashboard--schedule-refresh))
      ("tab_navigated"
       (run-hook-with-args 'ewd-tab-navigated-hook
                           (format "%s" (cdr (assq 'id params)))
                           (cdr (assq 'url params)))
       (ewd-dashboard--schedule-refresh))
      ("tab_reloaded"
       (run-hook-with-args 'ewd-tab-reloaded-hook
                           (format "%s" (cdr (assq 'id params))))
       (ewd-dashboard--schedule-refresh))
      ("tab_title_changed"
       (run-hook-with-args 'ewd-tab-title-changed-hook
                           (format "%s" (cdr (assq 'id params)))
                           (cdr (assq 'title params)))
       (ewd-dashboard--schedule-refresh))
      ("tab_visibility_changed"
       (run-hook-with-args 'ewd-tab-visibility-changed-hook
                           (format "%s" (cdr (assq 'id params)))
                           (cdr (assq 'visible params)))
       (ewd-dashboard--schedule-refresh))
      ("network_event"
       (run-hook-with-args 'ewd-network-event-hook
                           (format "%s" (cdr (assq 'id params)))
                           (cdr (assq 'event params))))
      ("js_message"
       (run-hook-with-args 'ewd-js-message-hook
                           (format "%s" (cdr (assq 'id params)))
                           (format "%s" (cdr (assq 'message_type params)))
                           (cdr (assq 'data params))))
      ("shutdown"
       (run-hooks 'ewd-shutdown-hook)
       (ewd-disconnect))
      (_
       (ewd--log "Unknown event: %s" method)))))

;;; --- JSON-RPC sending ---

(defun ewd--send-request (method params callback &optional error-callback)
  "Send JSON-RPC request with METHOD and PARAMS.
CALLBACK receives full response alist on success.
ERROR-CALLBACK receives error message string on failure.
Returns the request ID."
  (unless (ewd--connected-p)
    (user-error "EWD: Not connected. Run M-x ewd-connect first"))
  (unless ewd--client-id
    (error "EWD: client-id not set; run ewd-connect first"))
  (let* ((id (cl-incf ewd--request-id))
         (params-with-client
          (cons (cons 'client_id ewd--client-id)
                (or params '())))
         (request `((jsonrpc . "2.0")
                    (id . ,id)
                    (method . ,method)
                    (params . ,params-with-client)))
         (json-str (json-encode request)))
    (let ((timer (run-with-timer
                  ewd-request-timeout nil
                  (lambda ()
                    (when-let ((pending (gethash id ewd--pending-requests)))
                      (remhash id ewd--pending-requests)
                      (if error-callback
                          (funcall error-callback
                                   (format "Request timeout for method '%s'" method))
                        (message "EWD: Request timeout for method '%s'" method)))))))
      (puthash id
               (cons (lambda (msg)
                       (if-let* ((err (cdr (assq 'error msg)))
                                 (err-msg (cdr (assq 'message err))))
                           (if error-callback
                               (funcall error-callback err-msg)
                             (message "EWD error [%s]: %s" method err-msg))
                         (when callback
                           (funcall callback msg))))
                     timer)
               ewd--pending-requests))
    (ewd--log ">> %s" json-str)
    (process-send-string ewd--process (concat json-str "\n"))
    id))

;;; --- Programmatic API (async) ---

(defun ewd-create-tab-async (url callback &optional error-callback visible profile)
  "Create a new tab with URL. CALLBACK receives response alist.
VISIBLE if non-nil, shows the window.
PROFILE is optional profile name (defaults to `ewd-daemon-profile')."
  (ewd--send-request
   "create"
   `((url . ,url)
     (visible . ,(if visible t :json-false))
     (profile . ,(or profile ewd-daemon-profile)))
   callback
   error-callback))

(defun ewd-close-tab-async (tab-id callback &optional error-callback)
  "Close tab with TAB-ID. CALLBACK receives response alist."
  (ewd--send-request
   "close"
   `((id . ,tab-id))
   callback
   error-callback))

(defun ewd-list-tabs-async (callback &optional error-callback)
  "List all tabs. CALLBACK receives response alist with :result list."
  (ewd--send-request "list" nil callback error-callback))

(defun ewd-navigate-async (tab-id url callback &optional error-callback)
  "Navigate tab TAB-ID to URL."
  (ewd--send-request
   "navigate"
   `((id . ,tab-id) (url . ,url))
   callback
   error-callback))

(defun ewd-reload-async (tab-id callback &optional error-callback)
  "Reload tab TAB-ID."
  (ewd--send-request
   "reload"
   `((id . ,tab-id))
   callback
   error-callback))

(defun ewd-show-async (tab-id callback &optional error-callback)
  "Show window for tab TAB-ID."
  (ewd--send-request
   "show"
   `((id . ,tab-id))
   callback
   error-callback))

(defun ewd-hide-async (tab-id callback &optional error-callback)
  "Hide window for tab TAB-ID."
  (ewd--send-request
   "hide"
   `((id . ,tab-id))
   callback
   error-callback))

(defun ewd-eval-async (tab-id script callback &optional error-callback)
  "Evaluate JavaScript SCRIPT in tab TAB-ID.
CALLBACK receives response alist; extract result via `ewd--extract-eval-result'."
  (ewd--send-request
   "eval"
   `((id . ,tab-id) (script . ,script))
   callback
   error-callback))

(defun ewd-shutdown-async (callback &optional error-callback)
  "Shutdown the daemon gracefully."
  (ewd--send-request "shutdown" nil callback error-callback))

;;; --- Synchronous wrappers ---

(defun ewd--sync-request (async-fn &rest args)
  "Execute ASYNC-FN with ARGS synchronously. Returns result or signals error."
  (let ((result nil)
        (error-msg nil)
        (done nil))
    (apply async-fn
           (append args
                   (list (lambda (res) (setq result res done t))
                         (lambda (err) (setq error-msg err done t)))))
    (while (not done)
      (accept-process-output nil 0.1))
    (if error-msg
        (error "EWD: %s" error-msg)
      result)))

(defun ewd--extract-create-id (msg)
  "Extract tab ID from `create' response MSG."
  (cdr (assq 'id (cdr (assq 'result msg)))))

(defun ewd--extract-list-tabs (msg)
  "Extract tabs list from `list' response MSG."
  (cdr (assq 'result msg)))

(defun ewd--extract-eval-result (msg)
  "Extract JS evaluation result from `eval' response MSG."
  (cdr (assq 'result (cdr (assq 'result msg)))))

;;; --- Interactive commands ---

;;;###autoload
(defun ewd-create-tab (url &optional visible)
  "Create a new tab with URL. With prefix arg, make it visible."
  (interactive "sURL: \nP")
  (ewd-create-tab-async
   url
   (lambda (msg)
     (let ((id (ewd--extract-create-id msg)))
       (message "EWD: Created tab %s" id)))
   nil
   visible))

;;;###autoload
(defun ewd-list-tabs ()
  "List all tabs in daemon (alias for `ewd-dashboard')."
  (interactive)
  (ewd-dashboard))

;;;###autoload
(defun ewd-eval (tab-id script)
  "Evaluate SCRIPT in TAB-ID."
  (interactive
   (list (ewd--read-tab-id "Tab ID: ")
         (read-string "JavaScript: ")))
  (ewd-eval-async
   tab-id script
   (lambda (msg)
     (let ((result (ewd--extract-eval-result msg)))
       (message "EWD: Eval result: %s" result)))))

;;;###autoload
(defun ewd-close-tab (tab-id)
  "Close tab with TAB-ID."
  (interactive (list (ewd--read-tab-id "Close tab: ")))
  (ewd-close-tab-async
   tab-id
   (lambda (_msg) (message "EWD: Closed tab %s" tab-id))))

;;;###autoload
(defun ewd-navigate (tab-id url)
  "Navigate tab TAB-ID to URL."
  (interactive
   (list (ewd--read-tab-id "Navigate tab: ")
         (read-string "URL: ")))
  (ewd-navigate-async
   tab-id url
   (lambda (_msg) (message "EWD: Navigated %s → %s" tab-id url))))

;;;###autoload
(defun ewd-reload (tab-id)
  "Reload tab TAB-ID."
  (interactive (list (ewd--read-tab-id "Reload tab: ")))
  (ewd-reload-async
   tab-id
   (lambda (_msg) (message "EWD: Reloaded %s" tab-id))))

;;;###autoload
(defun ewd-show (tab-id)
  "Show window for tab TAB-ID."
  (interactive (list (ewd--read-tab-id "Show tab: ")))
  (ewd-show-async
   tab-id
   (lambda (_msg) (message "EWD: Showing %s" tab-id))))

;;;###autoload
(defun ewd-hide (tab-id)
  "Hide window for tab TAB-ID."
  (interactive (list (ewd--read-tab-id "Hide tab: ")))
  (ewd-hide-async
   tab-id
   (lambda (_msg) (message "EWD: Hiding %s" tab-id))))

;;;###autoload
(defun ewd-shutdown ()
  "Shutdown the daemon gracefully."
  (interactive)
  (when (yes-or-no-p "Shutdown ewd-daemon? ")
    (ewd-shutdown-async
     (lambda (_msg) (message "EWD: Daemon shutdown initiated")))))

;;; --- Helpers ---

(defun ewd--read-tab-id (prompt)
  "Read tab ID with completion from PROMPT."
  (let* ((tabs (ewd--extract-list-tabs
                (ewd--sync-request #'ewd-list-tabs-async)))
         (choices (mapcar (lambda (tab)
                            (cons (format "%s — %s"
                                          (cdr (assq 'id tab))
                                          (or (cdr (assq 'title tab)) "(no title)"))
                                  (cdr (assq 'id tab))))
                          tabs)))
    (if choices
        (cdr (assoc (completing-read prompt choices nil t) choices))
      (error "No tabs available"))))

;;; --- Dashboard ---

(defvar ewd-dashboard-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "g") #'ewd-dashboard-refresh)
    (define-key map (kbd "k") #'ewd-dashboard-close)
    (define-key map (kbd "o") #'ewd-dashboard-browse-url)
    (define-key map (kbd "RET") #'ewd-dashboard-browse-url)
    (define-key map (kbd "s") #'ewd-dashboard-show)
    (define-key map (kbd "h") #'ewd-dashboard-hide)
    (define-key map (kbd "e") #'ewd-dashboard-eval)
    (define-key map (kbd "t") #'ewd-create-tab)
    (define-key map (kbd "c") #'ewd-dashboard-copy-id)
    (define-key map (kbd "q") #'quit-window)
    map)
  "Keymap for `ewd-dashboard-mode'.")

(define-derived-mode ewd-dashboard-mode special-mode "EWD-Dashboard"
  "Major mode for EWD dashboard.

\\{ewd-dashboard-mode-map}"
  (setq buffer-read-only t))

(defun ewd-dashboard--truncate (str max-len)
  "Truncate STR to MAX-LEN characters, adding ellipsis if needed."
  (cond
   ((null str) "")
   ((> (length str) max-len)
    (concat (substring str 0 (1- max-len)) "…"))
   (t str)))

(defun ewd-dashboard--short-uuid (uuid len)
  "Return first LEN characters of UUID string."
  (if (and uuid (> (length uuid) len))
      (substring uuid 0 len)
    (or uuid "")))

(defun ewd-dashboard--format-tab (tab)
  "Format TAB for dashboard display with aligned columns.
Columns: URL | VISIBLE | TITLE | ID"
  (let* ((id (cdr (assq 'id tab)))
         (title (cdr (assq 'title tab)))
         (url (cdr (assq 'url tab)))
         (visible (cdr (assq 'visible tab)))
         (vis-str (if visible "yes" "no "))
         (url-str (ewd-dashboard--truncate url ewd-dashboard-url-width))
         (title-str (ewd-dashboard--truncate title ewd-dashboard-title-width))
         (id-str (ewd-dashboard--short-uuid id ewd-dashboard-id-width))
         (line (format (format "%%-%ds | %%-5s | %%-%ds | %%-%ds"
                               ewd-dashboard-url-width
                               ewd-dashboard-title-width
                               ewd-dashboard-id-width)
                       url-str vis-str title-str id-str))
         (face (if visible 'ewd-dashboard-visible-face 'default)))
    (propertize (concat line "\n")
                'mouse-face 'highlight
                'face face
                'help-echo (format "ID: %s\nURL: %s\nTitle: %s\nPress 'c' to copy ID"
                                   id url title)
                'ewd-tab-id id
                'ewd-tab-url url)))

(defun ewd-dashboard--tab-at-point ()
  "Return tab ID at point, or nil if point is not on a tab row."
  (get-text-property (line-beginning-position) 'ewd-tab-id))

(defun ewd-dashboard--url-at-point ()
  "Return URL at point, or nil if point is not on a tab row."
  (get-text-property (line-beginning-position) 'ewd-tab-url))

(defun ewd-dashboard--require-tab-at-point ()
  "Return tab ID at point, or signal user-error if none."
  (or (ewd-dashboard--tab-at-point)
      (user-error "No tab at point")))

(defun ewd-dashboard--refresh ()
  "Refresh dashboard content."
  (when (get-buffer ewd--dashboard-buffer-name)
    (with-current-buffer ewd--dashboard-buffer-name
      (let ((inhibit-read-only t)
            (point (point)))
        (erase-buffer)
        (if (not (ewd--connected-p))
            (progn
              (insert (propertize "Not connected to ewd-daemon.\n\n" 'face 'bold))
              (insert (propertize "Actions:\n" 'face 'bold))
              (insert "  C-c w c  Connect to daemon (auto-starts if needed)\n")
              (insert "  g        Refresh this list\n")
              (insert "  q        Quit dashboard window\n\n")
              (insert (propertize "Press 'C-c w c' or run M-x ewd-connect to start.\n"
                                  'face 'italic)))
          (if (and ewd--dashboard-cache (> (length ewd--dashboard-cache) 0))
              (progn
                ;; Column headers
                (insert (propertize
                         (format (format "%%-%ds | %%-5s | %%-%ds | %%-%ds"
                                         ewd-dashboard-url-width
                                         ewd-dashboard-title-width
                                         ewd-dashboard-id-width)
                                 "URL" "VIS" "TITLE" "ID")
                         'face 'bold))
                (insert "\n")
                ;; Separator
                (insert (make-string (+ ewd-dashboard-url-width 3
                                        5 3
                                        ewd-dashboard-title-width 3
                                        ewd-dashboard-id-width)
                                     ?-) "\n")
                ;; Tab rows
                (dolist (tab ewd--dashboard-cache)
                  (insert (ewd-dashboard--format-tab tab)))
                ;; Footer
                (insert "\n")
                (insert (propertize "Keybindings:\n" 'face 'bold))
                (insert "  g    Refresh list\n")
                (insert "  k    Close tab at point\n")
                (insert "  o    Open tab URL in external browser\n")
                (insert "  RET  Open tab URL in external browser\n")
                (insert "  s    Show tab in GTK window\n")
                (insert "  h    Hide tab (make headless)\n")
                (insert "  e    Evaluate JS in tab at point\n")
                (insert "  t    Create new tab\n")
                (insert "  c    Copy full tab ID to kill-ring\n")
                (insert "  q    Quit dashboard window\n"))
            ;; Empty state
            (insert (propertize "No tabs open.\n\n" 'face 'bold))
            (insert (propertize "Actions:\n" 'face 'bold))
            (insert "  t    Create a new tab\n")
            (insert "  g    Refresh this list\n")
            (insert "  q    Quit dashboard window\n")))
        (goto-char (min point (point-max)))))))

(defun ewd-dashboard--schedule-refresh ()
  "Schedule a debounced dashboard refresh."
  (run-with-timer 0.1 nil #'ewd-dashboard--refresh-cache))

(defun ewd-dashboard--refresh-cache ()
  "Refresh cache and update dashboard."
  (when (ewd--connected-p)
    (ewd-list-tabs-async
     (lambda (msg)
       (setq ewd--dashboard-cache (ewd--extract-list-tabs msg))
       (ewd-dashboard--refresh))
     (lambda (_err)
       (setq ewd--dashboard-cache nil)
       (ewd-dashboard--refresh)))))

;;;###autoload
(defun ewd-dashboard ()
  "Show EWD dashboard buffer."
  (interactive)
  (let ((buf (get-buffer-create ewd--dashboard-buffer-name)))
    (with-current-buffer buf
      (unless (derived-mode-p 'ewd-dashboard-mode)
        (ewd-dashboard-mode)))
    (display-buffer buf)
    (ewd-dashboard--refresh)
    (ewd-dashboard--refresh-cache)))

(defun ewd-dashboard-refresh ()
  "Refresh dashboard manually."
  (interactive)
  (ewd-dashboard--refresh-cache))

(defun ewd-dashboard-close ()
  "Close tab at point."
  (interactive)
  (let ((tab-id (ewd-dashboard--require-tab-at-point)))
    (when (yes-or-no-p (format "Close tab %s? " (ewd-dashboard--short-uuid tab-id 8)))
      (ewd-close-tab-async tab-id
                           (lambda (_msg) (message "EWD: Closed %s" tab-id))))))

(defun ewd-dashboard-show ()
  "Show tab at point in GTK window."
  (interactive)
  (let ((tab-id (ewd-dashboard--require-tab-at-point)))
    (ewd-show-async tab-id
                    (lambda (_msg) (message "EWD: Showing %s" tab-id)))))

(defun ewd-dashboard-hide ()
  "Hide tab at point (convert to headless)."
  (interactive)
  (let ((tab-id (ewd-dashboard--require-tab-at-point)))
    (ewd-hide-async tab-id
                    (lambda (_msg) (message "EWD: Hidden %s" tab-id)))))

(defun ewd-dashboard-browse-url ()
  "Open URL of tab at point in external browser."
  (interactive)
  (let ((url (ewd-dashboard--url-at-point)))
    (if url
        (progn
          (browse-url url)
          (message "EWD: Opened %s in external browser" url))
      (user-error "No tab at point"))))

(defun ewd-dashboard-eval ()
  "Evaluate JavaScript in tab at point."
  (interactive)
  (let ((tab-id (ewd-dashboard--require-tab-at-point)))
    (let ((script (read-string (format "JS in %s: " (ewd-dashboard--short-uuid tab-id 8)))))
      (ewd-eval-async
       tab-id script
       (lambda (msg)
         (let ((result (ewd--extract-eval-result msg)))
           (message "EWD [%s]: %s" (ewd-dashboard--short-uuid tab-id 8) result)))))))

(defun ewd-dashboard-copy-id ()
  "Copy full tab ID at point to kill-ring."
  (interactive)
  (let ((tab-id (ewd-dashboard--require-tab-at-point)))
    (kill-new tab-id)
    (message "EWD: Copied tab ID to kill-ring: %s" tab-id)))

;;; --- Cleanup ---

(defun ewd--cleanup-on-exit ()
  "Silently shutdown daemon and cleanup all resources on Emacs exit."
  (when (ewd--connected-p)
    (ignore-errors
      (let ((request (json-encode `((jsonrpc . "2.0")
                                    (id . 999999)
                                    (method . "shutdown")
                                    (params . ((client_id . ,ewd--client-id)))))))
        (process-send-string ewd--process (concat request "\n"))
        (accept-process-output ewd--process 0.5))))
  (when (and ewd--process (process-live-p ewd--process))
    (ignore-errors (delete-process ewd--process)))
  (setq ewd--process nil)
  (maphash (lambda (_id data)
             (let ((timer (cdr data)))
               (when timer (ignore-errors (cancel-timer timer)))))
           ewd--pending-requests)
  (clrhash ewd--pending-requests)
  (when (and ewd--socket-path (file-exists-p ewd--socket-path))
    (ignore-errors (delete-file ewd--socket-path)))
  (setq ewd--socket-path nil)
  (dolist (buf-name (list ewd--dashboard-buffer-name "*ewd-debug*"))
    (when-let ((buf (get-buffer buf-name)))
      (ignore-errors (kill-buffer buf))))
  (setq ewd--client-id nil
        ewd--read-buffer ""
        ewd--dashboard-cache nil
        ewd--request-id 0))

(add-hook 'kill-emacs-hook #'ewd--cleanup-on-exit)

;;; --- Global Keybindings ---

(defvar ewd-global-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "c") #'ewd-connect)
    (define-key map (kbd "d") #'ewd-dashboard)
    (define-key map (kbd "t") #'ewd-create-tab)
    (define-key map (kbd "e") #'ewd-eval)
    (define-key map (kbd "k") #'ewd-close-tab)
    (define-key map (kbd "n") #'ewd-navigate)
    (define-key map (kbd "r") #'ewd-reload)
    (define-key map (kbd "s") #'ewd-show)
    (define-key map (kbd "x") #'ewd-hide)
    (define-key map (kbd "q") #'ewd-shutdown)
    map)
  "Keymap for EWD commands after `C-c w'.")

(define-key global-map (kbd "C-c w") ewd-global-map)

(provide 'ewd)
;;; ewd.el ends here
