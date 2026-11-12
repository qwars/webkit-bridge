;;; ewd-dashboard.el --- Dashboard UI for Emacs Web Driver  -*- lexical-binding: t; -*-

;; Copyright (C) 2026
;; Author: EWD Project
;; Version: 0.9.0
;; Package-Requires: ((emacs "28.1"))
;; Keywords: comm, hypermedia, tools

;;; Commentary:

;; Dashboard UI for EWD — displays list of open tabs in *ewd* buffer.
;; Provides interactive commands for tab management.

;;; Code:

(require 'browse-url)

;;; --- Forward declarations ---
(declare-function ewd--connected-p "ewd")
(declare-function ewd-create-tab "ewd")
(declare-function ewd-list-tabs-async "ewd")
(declare-function ewd--extract-list-tabs "ewd")
(declare-function ewd-close-tab-async "ewd")
(declare-function ewd-show-async "ewd")
(declare-function ewd-hide-async "ewd")
(declare-function ewd-eval-async "ewd")
(declare-function ewd--extract-eval-result "ewd")

;;; --- Customization ---
(defcustom ewd-dashboard-enabled t
  "If non-nil, enable dashboard UI."
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
(defvar ewd--dashboard-buffer-name "*ewd*"
  "Name of the dashboard buffer.")

(defvar ewd--dashboard-cache nil
  "Cached list of tabs for dashboard rendering.")

;;; --- Hooks ---
(defvar ewd-dashboard-after-refresh-hook nil
  "Hook run after dashboard buffer is refreshed.
Called with no arguments, in the dashboard buffer context.")

;;; --- Mode ---
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

;;; --- Helpers ---
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

;;; --- Rendering ---
(defun ewd-dashboard--refresh ()
  "Refresh dashboard content."
  (when ewd-dashboard-enabled
    (when (get-buffer ewd--dashboard-buffer-name)
      (with-current-buffer ewd--dashboard-buffer-name
        (let ((inhibit-read-only t)
              (point (point)))
          (erase-buffer)
          (if (not (ewd--connected-p))
              (progn
                (insert (propertize "Not connected to ewd-daemon.\n" 'face 'bold))
                (insert (propertize "Actions:\n" 'face 'bold))
                (insert "  C-c w c  Connect to daemon (auto-starts if needed)\n")
                (insert "  g        Refresh this list\n")
                (insert "  q        Quit dashboard window\n")
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
              (insert (propertize "No tabs open.\n" 'face 'bold))
              (insert (propertize "Actions:\n" 'face 'bold))
              (insert "  t    Create a new tab\n")
              (insert "  g    Refresh this list\n")
              (insert "  q    Quit dashboard window\n")))
          (goto-char (min point (point-max)))
          (run-hooks 'ewd-dashboard-after-refresh-hook))))))

(defun ewd-dashboard--schedule-refresh ()
  "Schedule a debounced dashboard refresh."
  (when ewd-dashboard-enabled
    (run-with-timer 0.1 nil #'ewd-dashboard--refresh-cache)))

(defun ewd-dashboard--refresh-cache ()
  "Refresh cache and update dashboard."
  (when (and ewd-dashboard-enabled (ewd--connected-p))
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
  (unless ewd-dashboard-enabled
    (user-error "Dashboard is disabled. Set ewd-dashboard-enabled to t"))
  (let ((buf (get-buffer-create ewd--dashboard-buffer-name)))
    (with-current-buffer buf
      (unless (derived-mode-p 'ewd-dashboard-mode)
        (ewd-dashboard-mode)))
    (display-buffer buf)
    (ewd-dashboard--refresh)
    (ewd-dashboard--refresh-cache)))

;;; --- Interactive commands ---
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

;;;###autoload
(defun ewd-list-tabs ()
  "List all tabs in daemon (alias for `ewd-dashboard')."
  (interactive)
  (ewd-dashboard))

(provide 'ewd-dashboard)

;;; ewd-dashboard.el ends here
