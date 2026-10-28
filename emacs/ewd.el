;;; ewd.el --- Emacs WebKit Daemon Client -*- lexical-binding: t; -*-

;; Author: Alexandr Selunin <aka.qwars@gmail.com>
;; Version: 0.1.0
;; Package-Requires: ((emacs "30.1"))
;; Keywords: webkit, browser, automation

;;; Commentary:
;; Emacs client for EWD (Emacs WebKit Daemon) controlled via Unix socket
;; using JSON-RPC 2.0 protocol. Fully compatible with Emacs 30+.
;; Features: safe read-only buffer updates, reactive UI, and full command set.

;;; Code:

(require 'json)
(require 'cl-lib)

;;; Customization

(defgroup ewd nil
  "Emacs WebKit Daemon client."
  :group 'external
  :prefix "ewd-")

(defcustom ewd-socket-path nil
  "Path to the EWD daemon Unix socket."
  :type '(choice (const :tag "Auto-detect" nil)
                 (string :tag "Custom path")))

(defcustom ewd-default-visible nil
  "Default visibility for new tabs."
  :type 'boolean)

(defcustom ewd-auto-connect t
  "Automatically connect to daemon when sending first request."
  :type 'boolean)

;;; Internal Variables

(defvar ewd--client nil
  "Network process object for daemon connection.")

(defvar ewd--buffer nil
  "Buffer for accumulating incoming data from daemon.")

(defvar ewd--request-id 0
  "Counter for generating unique request IDs.")

(defvar ewd--pending-requests (make-hash-table :test 'equal)
  "Hash table mapping request IDs to callback functions.")

(defvar ewd--event-handlers (make-hash-table :test 'equal)
  "Hash table mapping event types to handler functions.")

(defvar ewd--tabs-buffer-name "*ewd-tabs*"
  "Name of the buffer for displaying tabs list.")

(defvar ewd--my-tabs nil
  "List of tab IDs created by this specific Emacs instance.")

;;; Connection Management

(defun ewd--get-socket-path ()
  "Return the socket path, auto-detecting if necessary."
  (or ewd-socket-path
      (let ((xdg-runtime (getenv "XDG_RUNTIME_DIR")))
        (if xdg-runtime
            (expand-file-name "webkit-bridge.sock" xdg-runtime)
          "/tmp/webkit-bridge.sock"))))

(defun ewd-connect ()
  "Connect to the EWD daemon."
  (interactive)
  (when ewd--client
    (user-error "Already connected to EWD daemon"))
  (let ((socket-path (ewd--get-socket-path)))
    (unless (file-exists-p socket-path)
      (user-error "Socket file does not exist: %s" socket-path))
    (setq ewd--buffer (generate-new-buffer " *ewd-buffer*"))
    (setq ewd--client
          (make-network-process
           :name "ewd-client"
           :family 'local
           :service socket-path
           :buffer ewd--buffer
           :filter #'ewd--filter
           :sentinel #'ewd--sentinel
           :nowait nil
           :coding 'utf-8-unix))
    (set-process-query-on-exit-flag ewd--client nil)
    (message "Connected to EWD daemon at %s" socket-path)))

(defun ewd-disconnect ()
  "Disconnect from the EWD daemon."
  (interactive)
  (when ewd--client
    (delete-process ewd--client)
    (setq ewd--client nil))
  (when ewd--buffer
    (kill-buffer ewd--buffer)
    (setq ewd--buffer nil))
  (clrhash ewd--pending-requests)
  (setq ewd--my-tabs nil))

(defun ewd--sentinel (_process event)
  "Handle connection state changes."
  (when (string-match-p "\\(closed\\|finished\\|connection broken\\|failed\\)" event)
    (setq ewd--client nil)
    (when ewd--buffer
      (kill-buffer ewd--buffer)
      (setq ewd--buffer nil))
    (setq ewd--my-tabs nil)))

(defun ewd--ensure-connected ()
  "Ensure connection to daemon, auto-connecting if needed."
  (unless ewd--client
    (if ewd-auto-connect
        (ewd-connect)
      (user-error "Not connected to EWD daemon. Run M-x ewd-connect first"))))

(defun ewd--cleanup-on-exit ()
  "Clean up resources and close owned tabs when Emacs exits."
  (when ewd--client
    (dolist (tab-id ewd--my-tabs)
      (let* ((req-id (cl-incf ewd--request-id))
             (request (list (cons "jsonrpc" "2.0")
                            (cons "method" "tab.close")
                            (cons "params" (list (cons "id" tab-id)))
                            (cons "id" req-id)))
             (json-str (json-encode request)))
        (ignore-errors
          (process-send-string ewd--client (concat json-str "\n")))))
    (sleep-for 0.1)
    (delete-process ewd--client)
    (setq ewd--client nil))
  (when ewd--buffer
    (kill-buffer ewd--buffer)
    (setq ewd--buffer nil))
  (clrhash ewd--pending-requests)
  (setq ewd--my-tabs nil))

;;; JSON-RPC Protocol

(defun ewd--send-request (method params callback)
  "Send a JSON-RPC request to the daemon."
  (ewd--ensure-connected)
  (cl-incf ewd--request-id)
  (let* ((req-id ewd--request-id)
         (request (list (cons "jsonrpc" "2.0")
                        (cons "method" method)
                        (cons "params" params)
                        (cons "id" req-id))))
    (puthash req-id callback ewd--pending-requests)
    (let ((json-str (json-encode request)))
      (ignore-errors
        (process-send-string ewd--client (concat json-str "\n"))))))

(defun ewd--filter (_process output)
  "Process incoming data from the daemon."
  (with-current-buffer ewd--buffer
    (goto-char (point-max))
    (insert output)
    (goto-char (point-min))
    (while (search-forward "\n" nil t)
      (let ((line (buffer-substring-no-properties (point-min) (1- (point)))))
        (delete-region (point-min) (point))
        (when (not (string-empty-p line))
          (ewd--handle-message line))))))

(defun ewd--handle-message (json-str)
  "Handle a complete JSON message from the daemon."
  (condition-case err
      (let* ((msg (json-parse-string json-str :object-type 'alist :array-type 'list))
             (msg-id (alist-get 'id msg))
             (msg-result (alist-get 'result msg))
             (msg-error (alist-get 'error msg))
             (msg-method (alist-get 'method msg))
             (msg-params (alist-get 'params msg)))
        (cond
         (msg-id
          (let ((callback (gethash msg-id ewd--pending-requests)))
            (when callback
              (remhash msg-id ewd--pending-requests)
              (if msg-error
                  (funcall callback nil (alist-get 'message msg-error))
                (funcall callback msg-result nil)))))
         (msg-method
          (let ((method-str (if (stringp msg-method) msg-method (symbol-name msg-method))))
            (ewd--dispatch-event method-str msg-params)))))
    (error
     (message "EWD: Failed to parse JSON: %s" (error-message-string err)))))

(defun ewd--dispatch-event (method params)
  "Dispatch an event notification to registered handlers."
  (let ((handlers (gethash method ewd--event-handlers)))
    (dolist (handler handlers)
      (condition-case err
          (funcall handler params)
        (error
         (message "EWD: Event handler error: %s" (error-message-string err)))))))

;;; Event Subscription

(defun ewd-subscribe (event-type handler)
  "Subscribe to an event type from the daemon."
  (let ((handlers (gethash event-type ewd--event-handlers)))
    (puthash event-type (cons handler handlers) ewd--event-handlers)))

(defun ewd-unsubscribe (event-type handler)
  "Unsubscribe a handler from an event type."
  (let ((handlers (gethash event-type ewd--event-handlers)))
    (puthash event-type (remove handler handlers) ewd--event-handlers)))

;;; Interactive Commands

(defun ewd-create-tab (url &optional visible)
  "Create a new tab with URL."
  (interactive
   (list (read-string "URL: ")
         current-prefix-arg))
  (let ((vis (or visible ewd-default-visible)))
    (ewd--send-request
     "tab.create"
     (list (cons "url" url)
           (cons "visible" vis))
     (lambda (result error)
       (if error
           (message "EWD: Failed to create tab: %s" error)
         (let ((tab-id (alist-get 'id result)))
           (push tab-id ewd--my-tabs)
           (message "EWD: Tab created with ID %s" tab-id)))))))

(defun ewd-close-tab (tab-id)
  "Close a tab by TAB-ID."
  (interactive
   (list (read-string "Tab ID to close: ")))
  (unless (member tab-id ewd--my-tabs)
    (user-error "EWD: Cannot close tab %s. Not created by this instance." tab-id))
  (ewd--send-request
   "tab.close"
   (list (cons "id" tab-id))
   (lambda (result error)
     (if error
         (message "EWD: Failed to close tab: %s" error)
       (progn
         (setq ewd--my-tabs (remove tab-id ewd--my-tabs))
         (message "EWD: Tab %s closed" tab-id))))))

(defun ewd-navigate (tab-id url)
  "Navigate TAB-ID to a new URL."
  (interactive
   (list (read-string "Tab ID: ")
         (read-string "New URL: ")))
  (unless (member tab-id ewd--my-tabs)
    (user-error "EWD: Cannot navigate tab %s. Not created by this instance." tab-id))
  (ewd--send-request
   "tab.navigate"
   (list (cons "id" tab-id)
         (cons "url" url))
   (lambda (result error)
     (if error
         (message "EWD: Failed to navigate: %s" error)
       (message "EWD: Navigated to %s" url)))))

(defun ewd-reload (tab-id)
  "Reload TAB-ID."
  (interactive
   (list (read-string "Tab ID to reload: ")))
  (unless (member tab-id ewd--my-tabs)
    (user-error "EWD: Cannot reload tab %s. Not created by this instance." tab-id))
  (ewd--send-request
   "tab.reload"
   (list (cons "id" tab-id))
   (lambda (result error)
     (if error
         (message "EWD: Failed to reload: %s" error)
       (message "EWD: Tab %s reloaded" tab-id)))))

(defun ewd-eval (tab-id script)
  "Evaluate JavaScript in TAB-ID."
  (interactive
   (list (read-string "Tab ID: ")
         (read-string "JavaScript: ")))
  (unless (member tab-id ewd--my-tabs)
    (user-error "EWD: Cannot evaluate JS in tab %s. Not created by this instance." tab-id))
  (ewd--send-request
   "tab.eval"
   (list (cons "id" tab-id)
         (cons "script" script))
   (lambda (result error)
     (if error
         (message "EWD: Eval failed: %s" error)
       (let ((res (alist-get 'result result)))
         (message "EWD Eval Result: %s" res))))))

(defun ewd-list-tabs ()
  "Display list of all tabs in a dedicated buffer."
  (interactive)
  (ewd--send-request
   "tab.list"
   nil
   (lambda (result error)
     (if error
         (message "EWD: Failed to list tabs: %s" error)
       (ewd--display-tabs result)))))

(defun ewd-shutdown ()
  "Shutdown the EWD daemon."
  (interactive)
  (when (yes-or-no-p "Shutdown EWD daemon? ")
    (ewd--send-request
     "daemon.shutdown"
     nil
     (lambda (result error)
       (if error
           (message "EWD: Failed to shutdown: %s" error)
         (message "EWD: Daemon shutdown initiated")
         (ewd-disconnect))))))

;;; Helper Functions


(defun ewd--display-tabs (tabs)
  "Display TABS in the dedicated buffer.
Uses inhibit-read-only to safely update a special-mode buffer."
  (let ((buf (get-buffer-create ewd--tabs-buffer-name)))
    (with-current-buffer buf
      (let ((inhibit-read-only t))
        (erase-buffer)
        (insert (format "EWD Tabs (%d total)\n" (if (listp tabs) (length tabs) 0)))
        (insert (make-string 60 ?-) "\n\n")
        (when (listp tabs)
          (dolist (tab tabs)
            (when (listp tab)
              (let* ((tab-id (alist-get 'id tab))
                     (url (alist-get 'url tab))
                     (title (alist-get 'title tab))
                     (owner (alist-get 'owner tab))
                     (owned (member tab-id ewd--my-tabs)))
                (insert (format "%s %-36s  %s\n"
                                (if owned "*" " ")
                                (or tab-id "unknown")
                                (or title "(no title)")))
                (insert (format "  URL: %s [owner: %s]\n\n"
                                (or url "unknown")
                                (or owner "unknown")))))))
        (ewd-tabs-buffer-mode)
        (goto-char (point-min))))
    (pop-to-buffer buf)))

;;; Tabs Buffer Mode

(defvar ewd-tabs-buffer-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "g") #'ewd-list-tabs)
    (define-key map (kbd "q") #'quit-window)
    (define-key map (kbd "d") #'ewd-close-tab-at-point)
    (define-key map (kbd "e") #'ewd-eval-tab-at-point)
    map)
  "Keymap for `ewd-tabs-buffer-mode'.")

(define-derived-mode ewd-tabs-buffer-mode special-mode "EWD-Tabs"
  "Major mode for displaying EWD tabs list.
\\{ewd-tabs-buffer-mode-map}"
  (setq buffer-read-only t))

(defun ewd-close-tab-at-point ()
  "Close the tab at point."
  (interactive)
  (save-excursion
    (beginning-of-line)
    (when (looking-at "^\\*? *\\([a-f0-9-]+\\)")
      (let ((tab-id (match-string 1)))
        (ewd-close-tab tab-id)
        (run-at-time 0.5 nil #'ewd-list-tabs)))))

(defun ewd-eval-tab-at-point (script)
  "Evaluate JavaScript in the tab at point."
  (interactive "sJavaScript: ")
  (save-excursion
    (beginning-of-line)
    (when (looking-at "^\\*? *\\([a-f0-9-]+\\)")
      (let ((tab-id (match-string 1)))
        (ewd-eval tab-id script)
        (run-at-time 0.5 nil #'ewd-list-tabs)))))

;;; Event Handlers

(defun ewd--on-tab-title-changed (params)
  "Handle tab.title_changed event."
  (let ((tab-id (alist-get 'id params))
        (title (alist-get 'title params)))
    (message "EWD: Tab %s title changed to: %s" tab-id title)
    (when (get-buffer ewd--tabs-buffer-name)
      (ewd-list-tabs))))

(defun ewd--on-tab-created (_params)
  "Handle tab.created event."
  (message "EWD: New tab created")
  (when (get-buffer ewd--tabs-buffer-name)
    (ewd-list-tabs)))

(defun ewd--on-tab-closed (params)
  "Handle tab.closed event."
  (let ((tab-id (alist-get 'id params)))
    (when tab-id
      (setq ewd--my-tabs (remove tab-id ewd--my-tabs))))
  (message "EWD: Tab closed")
  (when (get-buffer ewd--tabs-buffer-name)
    (ewd-list-tabs)))

;;; Initialization

(defun ewd-init ()
  "Initialize EWD client and register default event handlers."
  (clrhash ewd--event-handlers)
  (ewd-subscribe "tab.title_changed" #'ewd--on-tab-title-changed)
  (ewd-subscribe "tab.created" #'ewd--on-tab-created)
  (ewd-subscribe "tab.closed" #'ewd--on-tab-closed)
  (add-hook 'kill-emacs-hook #'ewd--cleanup-on-exit))

(ewd-init)

;;; Keybindings

(global-set-key (kbd "C-c w c") #'ewd-create-tab)
(global-set-key (kbd "C-c w k") #'ewd-close-tab)
(global-set-key (kbd "C-c w l") #'ewd-list-tabs)
(global-set-key (kbd "C-c w e") #'ewd-eval)
(global-set-key (kbd "C-c w n") #'ewd-navigate)
(global-set-key (kbd "C-c w r") #'ewd-reload)
(global-set-key (kbd "C-c w s") #'ewd-shutdown)
(global-set-key (kbd "C-c w C") #'ewd-connect)
(global-set-key (kbd "C-c w D") #'ewd-disconnect)

(provide 'ewd)

;;; ewd.el ends here
