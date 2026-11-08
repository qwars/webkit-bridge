;;; ewd.el --- Emacs WebKit Daemon client -*- lexical-binding: t; -*-

;; Copyright (C) 2026
;; Author: Alexandr Selunin <aka.qwars@gmail.com> @qwars
;; Version: 0.2.0
;; Package-Requires: emacs 30+
;; Keywords: comm, hypermedia, tools

;;; Commentary:

;; Client for Emacs WebKit Daemon (EWD).
;; Provides commands to manage WebKit tabs via a daemon over Unix socket.
;;
;; The daemon pushes `tabs_changed' notifications whenever any tab
;; state changes. The client reacts by re-fetching the tab list and
;; redrawing the buffer while preserving cursor position.
;;
;; Commands:
;;   M-x ewd-connect           - Connect to the daemon
;;   M-x ewd-disconnect        - Disconnect from the daemon
;;   M-x ewd-create-tab        - Create a new tab
;;   M-x ewd-list-tabs         - List all open tabs
;;   M-x ewd-close-tab         - Close a tab by ID
;;   M-x ewd-eval              - Evaluate JavaScript in a tab
;;   M-x ewd-navigate          - Navigate a tab to a new URL
;;   M-x ewd-reload            - Reload a tab
;;   M-x ewd-switch-to-tab     - Open tab URL in system browser
;;   M-x ewd-show-tab          - Show tab in GTK window
;;   M-x ewd-hide-tab          - Hide tab (convert to headless)
;;
;; Keybindings (global):
;;   C-c w c   ewd-connect
;;   C-c w d   ewd-disconnect
;;   C-c w t   ewd-create-tab
;;   C-c w l   ewd-list-tabs
;;   C-c w k   ewd-close-tab
;;   C-c w e   ewd-eval
;;   C-c w n   ewd-navigate
;;   C-c w r   ewd-reload
;;   C-c w o   ewd-switch-to-tab
;;   C-c w S   ewd-show-tab
;;   C-c w H   ewd-hide-tab
;;
;; In *ewd-tabs* buffer:
;;   g   refresh list
;;   k   close tab at point (works from any line of the tab block)
;;   o   open tab URL in browser (works from any line of the tab block)
;;   RET open tab URL in browser (works from any line of the tab block)
;;   s   show tab at point in GTK window
;;   h   hide tab at point (convert to headless)
;;   q   quit window

;;; Code:

(require 'json)

;;; --- Customization ---
(defgroup ewd nil "Emacs WebKit Daemon client." :group 'external :prefix "ewd-")

(defcustom ewd-socket-path nil
  "Path to the EWD socket."
  :type '(choice (const :tag "Auto-detect" nil) (string :tag "Path"))
  :group 'ewd)

;;; --- Internal State ---
(defvar ewd--process nil "Network process for EWD communication.")
(defvar ewd--client-id nil "Unique client ID for this Emacs session (UUID v4).")
(defvar ewd--tabs '() "List of tabs known to this client: (ID URL TITLE OWNER VISIBLE).")
(defvar ewd--pending-requests (make-hash-table :test 'equal) "Hash table of pending requests.")
(defvar ewd--request-id 0 "Counter for JSON-RPC request IDs.")

;;; --- UUID Generation ---
(defun ewd--generate-uuid ()
  "Generate a UUID v4 string."
  (format "%04x%04x-%04x-4%03x-%04x-%04x%04x%04x"
          (random 65536) (random 65536) (random 65536) (random 4096)
          (logior (random 16384) 32768) (random 65536) (random 65536) (random 65536)))

;;; --- Socket Path Resolution ---
(defun ewd--get-socket-path ()
  "Return the path to the EWD socket."
  (or ewd-socket-path
      (getenv "EWD_SOCKET_PATH")
      (when-let ((xdg (getenv "XDG_RUNTIME_DIR")))
        (expand-file-name "webkit-bridge.sock" xdg))
      "/tmp/webkit-bridge.sock"))

;;; --- Initialization / Teardown ---
(defun ewd-connect ()
  "Initialize connection to the EWD daemon."
  (interactive)
  (unless ewd--client-id
    (setq ewd--client-id (ewd--generate-uuid))
    (message "EWD: Generated client ID: %s" ewd--client-id))
  (when ewd--process (delete-process ewd--process))
  (let ((socket-path (ewd--get-socket-path)))
    (setq ewd--process
          (make-network-process
           :name "ewd" :family 'local :service socket-path
           :buffer " *ewd-process*" :filter #'ewd--process-filter
           :sentinel #'ewd--process-sentinel :nowait nil))
    (set-process-query-on-exit-flag ewd--process nil)
    (message "EWD: Connected to %s (client %s)" socket-path ewd--client-id)))

(defun ewd-disconnect ()
  "Disconnect from the EWD daemon."
  (interactive)
  (when ewd--process
    (delete-process ewd--process)
    (setq ewd--process nil))
  (setq ewd--tabs '())
  (message "EWD: Disconnected"))

;;; --- Process Handling ---
(defun ewd--process-filter (process output)
  "Filter for EWD process."
  (with-current-buffer (process-buffer process)
    (goto-char (point-max))
    (insert output)
    (goto-char (point-min))
    (while (search-forward "\n" nil t)
      (let ((line (buffer-substring-no-properties (point-min) (1- (point)))))
        (delete-region (point-min) (point))
        (unless (string-empty-p line)
          (ewd--handle-message line))))))

(defun ewd--process-sentinel (process event)
  "Sentinel for EWD process."
  (message "EWD: Connection %s" (string-trim event))
  (setq ewd--process nil))

;;; --- Message Handling ---
(defun ewd--handle-message (line)
  "Handle a single JSON-RPC message LINE from the daemon."
  (condition-case err
      (let ((msg (json-read-from-string line)))
        (cond
         ((assq 'id msg)
          (let* ((id (cdr (assq 'id msg)))
                 (callback (gethash id ewd--pending-requests)))
            (when callback
              (remhash id ewd--pending-requests)
              (funcall callback msg))))
         ((assq 'method msg)
          (ewd--handle-notification msg))))
    (error (message "EWD: Error parsing message: %S" err))))

(defun ewd--handle-notification (msg)
  "Handle a JSON-RPC notification MSG from the daemon."
  (let ((method (cdr (assq 'method msg))))
    (when (and method (string= (format "%s" method) "tabs_changed"))
      (when (get-buffer-window "*ewd-tabs*" t)
        (ewd-list-tabs)))))

;;; --- JSON-RPC Communication ---
(defun ewd--send-request (method params callback)
  "Send a JSON-RPC request with METHOD and PARAMS."
  (unless ewd--process
    (user-error "EWD: Not connected. Run M-x ewd-connect first"))
  (let* ((id (cl-incf ewd--request-id))
         (params-with-client (cons (cons 'client_id ewd--client-id) (or params '())))
         (request `((jsonrpc . "2.0") (method . ,method) (params . ,params-with-client) (id . ,id)))
         (json-str (json-encode request)))
    (puthash id callback ewd--pending-requests)
    (process-send-string ewd--process (concat json-str "\n"))))

(defun ewd--handle-error (msg)
  "Handle a JSON-RPC error response MSG."
  (if-let* ((error (cdr (assq 'error msg)))
            (code (cdr (assq 'code error)))
            (message (cdr (assq 'message error))))
      (message "EWD: Error [%s]: %s" code message)
    (message "EWD: Unknown error: %S" msg)))

;;; --- Commands ---
(defun ewd-create-tab (url &optional visible)
  "Create a new tab with URL."
  (interactive "sURL: \nP")
  (ewd--send-request "create" `((url . ,url) (visible . ,(if visible t :json-false)))
    (lambda (msg)
      (if-let* ((result (cdr (assq 'result msg))) (id (cdr (assq 'id result))))
          (message "EWD: Created tab %s" id)
        (ewd--handle-error msg)))))

(defun ewd-list-tabs ()
  "List all open tabs and display them in `*ewd-tabs*' buffer."
  (interactive)
  (ewd--send-request "list" nil
    (lambda (msg)
      (if-let ((result (cdr (assq 'result msg))))
          (progn
            (setq ewd--tabs (ewd--parse-tabs result))
            (ewd--display-tabs))
        (ewd--handle-error msg)))))

(defun ewd--parse-tabs (result)
  "Parse tab list RESULT into a list of (ID URL TITLE OWNER VISIBLE) tuples."
  (when (arrayp result)
    (mapcar (lambda (item)
              (list (cdr (assq 'id item))
                    (cdr (assq 'url item))
                    (cdr (assq 'title item))
                    (cdr (assq 'owner item))
                    (cdr (assq 'visible item))))
            result)))

(defun ewd--display-tabs ()
  "Display the list of tabs in `*ewd-tabs*' buffer.
Preserves cursor position and highlights tabs based on visibility and ownership."
  (with-current-buffer (get-buffer-create "*ewd-tabs*")
    (let ((inhibit-read-only t)
          (current-tab-id nil))
      ;; 1. Запоминаем UUID таба под курсором перед очисткой буфера
      (save-excursion
        (end-of-line)
        (when (re-search-backward "\\[\\([a-f0-9]\\{8\\}-[a-f0-9]\\{4\\}-[a-f0-9]\\{4\\}-[a-f0-9]\\{4\\}-[a-f0-9]\\{12\\}\\)\\]" nil t)
          (setq current-tab-id (match-string 1))))

      (erase-buffer)

      ;; 2. Вставляем заголовок
      (insert (format "EWD Tabs (%d open, client %s)\n"
                      (length ewd--tabs)
                      (or ewd--client-id "none"))
              (make-string 60 ?=) "\n\n"
              "Legend: * = owned by this Emacs session\n"
              "        [V] = visible, [H] = hide\n"
              "        colored text = owned by this session\n\n")

      ;; 3. Вставляем табы
      (if (null ewd--tabs)
          (insert "No tabs open.\n")
        (dolist (tab ewd--tabs)
          (let* ((id (nth 0 tab))
                 (url (nth 1 tab))
                 (title (nth 2 tab))
                 (owner (nth 3 tab))
                 (visible (nth 4 tab))
                 (is-visible (and visible (not (eq visible :json-false))))
                 (is-owned (string-prefix-p owner ewd--client-id))
                 (marker (if is-owned "*" " "))
                 (vis-marker (if is-visible "[V]" "[H]"))
                 (text (format "%s %s [%s] %s\n  URL: %s\n  Owner: %s\n\n"
                               marker
                               vis-marker
                               id
                               (if (and title (not (string-empty-p title))) title "(no title)")
                               (or url "(no url)")
                               (or owner "(unknown)"))))
            (cond
             ((and is-owned is-visible)
              (insert (propertize text 'face 'font-lock-type-face)))
             (is-owned
              (insert text))
             (t
              (insert (propertize text 'face 'font-lock-comment-face)))))))

      ;; 4. Восстанавливаем курсор
      (goto-char (point-min))
      (if current-tab-id
          (progn
            ;; Ищем именно этот UUID в квадратных скобках
            (if (re-search-forward (format "\\[%s\\]" (regexp-quote current-tab-id)) nil t)
                (beginning-of-line) ;; Оставляем курсор в начале строки таба
              (goto-char (point-min))))
        (goto-char (point-min)))

      (ewd-tabs-mode)
      (display-buffer (current-buffer)))))

;;; --- Tabs Buffer Mode ---
(defvar ewd-tabs-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "g") #'ewd-list-tabs)
    (define-key map (kbd "k") #'ewd-close-tab-at-point)
    (define-key map (kbd "o") #'ewd-open-tab-at-point)
    (define-key map (kbd "RET") #'ewd-open-tab-at-point)
    (define-key map (kbd "s") #'ewd-show-tab-at-point)
    (define-key map (kbd "h") #'ewd-hide-tab-at-point)
    (define-key map (kbd "q") #'quit-window)
    map)
  "Keymap for `ewd-tabs-mode'.")

(define-derived-mode ewd-tabs-mode special-mode "EWD-Tabs"
  "Major mode for displaying EWD tabs.

\\{ewd-tabs-mode-map}"
  (setq buffer-read-only t)
  (setq truncate-lines t))

(defun ewd--get-tab-id-at-point ()
  "Надежно извлекает UUID таба под курсором или выше по буферу."
  (save-excursion
    (end-of-line)
    (when (re-search-backward "\\[\\([a-f0-9]\\{8\\}-[a-f0-9]\\{4\\}-[a-f0-9]\\{4\\}-[a-f0-9]\\{4\\}-[a-f0-9]\\{12\\}\\)\\]" nil t)
      (match-string 1))))

(defun ewd-close-tab-at-point ()
  "Close the tab at point in `*ewd-tabs*' buffer."
  (interactive)
  (let ((id (ewd--get-tab-id-at-point)))
    (when id (ewd-close-tab id))))

(defun ewd-open-tab-at-point ()
  "Open the URL of the tab at point in the system browser."
  (interactive)
  (let ((id (ewd--get-tab-id-at-point)))
    (when id
      (let* ((tab (assoc id ewd--tabs))
             (url (when tab (nth 1 tab))))
        (if url (browse-url url) (message "EWD: URL not found for tab %s" id))))))

(defun ewd-show-tab-at-point ()
  "Show the tab at point in a GTK window."
  (interactive)
  (let ((id (ewd--get-tab-id-at-point)))
    (when id (ewd-show-tab id))))

(defun ewd-hide-tab-at-point ()
  "Hide the tab at point (convert to headless)."
  (interactive)
  (let ((id (ewd--get-tab-id-at-point)))
    (when id (ewd-hide-tab id))))

(defun ewd-close-tab (id)
  "Close the tab with ID."
  (interactive (list (completing-read "Close tab: " (mapcar #'car ewd--tabs) nil t)))
  (ewd--send-request "close" `((id . ,id))
    (lambda (msg)
      (if (cdr (assq 'error msg)) (ewd--handle-error msg)
        (message "EWD: Closed tab %s" id)
        (ewd-list-tabs)))))

(defun ewd-show-tab (id)
  "Show the tab with ID in a GTK window."
  (interactive (list (completing-read "Show tab: " (mapcar #'car ewd--tabs) nil t)))
  (ewd--send-request "show" `((id . ,id))
    (lambda (msg)
      (if (cdr (assq 'error msg)) (ewd--handle-error msg)
        (message "EWD: Shown tab %s" id)
        (ewd-list-tabs)))))

(defun ewd-hide-tab (id)
  "Hide the tab with ID (convert to headless)."
  (interactive (list (completing-read "Hide tab: " (mapcar #'car ewd--tabs) nil t)))
  (ewd--send-request "hide" `((id . ,id))
    (lambda (msg)
      (if (cdr (assq 'error msg)) (ewd--handle-error msg)
        (message "EWD: Hidden tab %s" id)
        (ewd-list-tabs)))))

(defun ewd-eval (id script)
  "Evaluate SCRIPT in the tab with ID."
  (interactive (list (completing-read "Tab ID: " (mapcar #'car ewd--tabs) nil t) (read-string "JavaScript: ")))
  (ewd--send-request "eval" `((id . ,id) (script . ,script))
    (lambda (msg)
      (if-let* ((result (cdr (assq 'result msg))) (eval-result (cdr (assq 'result result))))
          (progn
            (message "EWD: Eval result: %s" eval-result)
            (with-current-buffer (get-buffer-create "*ewd-eval*")
              (let ((inhibit-read-only t))
                (erase-buffer)
                (insert (format "Eval in tab %s\n" id) (make-string 60 ?=) "\n\n"
                        "Script:\n" script "\n\n" "Result:\n" (format "%s" eval-result) "\n")
                (goto-char (point-min)) (special-mode) (display-buffer (current-buffer)))))
        (ewd--handle-error msg)))))

(defun ewd-navigate (id url)
  "Navigate tab with ID to URL."
  (interactive (list (completing-read "Tab ID: " (mapcar #'car ewd--tabs) nil t) (read-string "URL: ")))
  (ewd--send-request "navigate" `((id . ,id) (url . ,url))
    (lambda (msg)
      (if (cdr (assq 'error msg)) (ewd--handle-error msg)
        (message "EWD: Navigated to %s" url)
        (ewd-list-tabs)))))

(defun ewd-reload (id)
  "Reload the tab with ID."
  (interactive (list (completing-read "Reload tab: " (mapcar #'car ewd--tabs) nil t)))
  (ewd--send-request "reload" `((id . ,id))
    (lambda (msg)
      (if (cdr (assq 'error msg)) (ewd--handle-error msg)
        (message "EWD: Reloaded tab %s" id)
        (ewd-list-tabs)))))

(defun ewd-switch-to-tab (id)
  "Open the URL of tab with ID in the system browser."
  (interactive (list (completing-read "Switch to tab: " (mapcar #'car ewd--tabs) nil t)))
  (if-let ((tab (assoc id ewd--tabs)))
      (let ((url (nth 1 tab))) (browse-url url) (message "EWD: Opened %s in browser" url))
    (message "EWD: Tab %s not found. Run M-x ewd-list-tabs first." id)))

;;; --- Cleanup ---
(defun ewd--cleanup-on-exit ()
  "Clean up resources and close owned tabs when Emacs exits."
  (when (and ewd--process ewd--client-id ewd--tabs)
    (dolist (tab ewd--tabs)
      (let ((id (nth 0 tab)) (owner (nth 3 tab)))
        (when (string-prefix-p owner ewd--client-id)
          (let* ((req `((jsonrpc . "2.0") (method . "close")
                        (params . ((client_id . ,ewd--client-id) (id . ,id))) (id . 999)))
                 (json-str (concat (json-encode req) "\n")))
            (process-send-string ewd--process json-str)))))
    (sleep-for 0.1)
    (delete-process ewd--process)
    (setq ewd--process nil)
    (when (get-buffer " *ewd-process*") (kill-buffer " *ewd-process*"))))

(add-hook 'kill-emacs-hook #'ewd--cleanup-on-exit)

;;; --- Keybindings ---
(global-set-key (kbd "C-c w c") #'ewd-connect)
(global-set-key (kbd "C-c w d") #'ewd-disconnect)
(global-set-key (kbd "C-c w t") #'ewd-create-tab)
(global-set-key (kbd "C-c w l") #'ewd-list-tabs)
(global-set-key (kbd "C-c w k") #'ewd-close-tab)
(global-set-key (kbd "C-c w e") #'ewd-eval)
(global-set-key (kbd "C-c w n") #'ewd-navigate)
(global-set-key (kbd "C-c w r") #'ewd-reload)
(global-set-key (kbd "C-c w o") #'ewd-switch-to-tab)
(global-set-key (kbd "C-c w S") #'ewd-show-tab)
(global-set-key (kbd "C-c w H") #'ewd-hide-tab)

(provide 'ewd)
;;; ewd.el ends here
