;;; ewd-debug.el --- Debug logging for Emacs Web Driver  -*- lexical-binding: t; -*-

;; Copyright (C) 2026
;; Author: EWD Project
;; Version: 0.9.0
;; Package-Requires: ((emacs "28.1"))
;; Keywords: comm, tools

;;; Commentary:
;; Debug logging grouped by tabs with deferred batched insertion.
;; Records are queued and flushed into the buffer when Emacs is idle,
;; using markers for section locations. Fold state, cursor position and
;; scroll position are captured once before the flush and restored once
;; after, so they never drift.

;;; Code:

(require 'org)
(require 'url-parse)
(require 'browse-url)

(unless (derived-mode-p 'org-mode)
  (org-mode)
  (setq-local browse-url-browser-function
    #'browse-url-xdg-open)
  (setq-local org-return-follows-link t))

;;; --- Variables ---

(defvar ewd--debug-buffer-name "*ewd-debug*"
  "Name of the debug buffer.")

(defvar ewd--tab-urls (make-hash-table :test 'equal)
  "Map tab-id to URL for header formatting.")

(defvar ewd--tab-titles (make-hash-table :test 'equal)
  "Map tab-id to title for header formatting.")

(defvar ewd--debug-pending nil
  "Queue of pending log records, each (TAB-ID . LINE).")

(defvar ewd--debug-flush-timer nil
  "Idle timer for deferred flush, or nil.")

(defvar ewd--debug-flush-delay 0.3
  "Idle delay in seconds before flushing pending records.")

(defvar ewd--debug-heading-markers (make-hash-table :test 'equal)
  "Map tab-id key to marker at the section heading line.")

(defvar ewd--debug-insert-markers (make-hash-table :test 'equal)
  "Map tab-id key to marker at the insertion point before #+END_EXAMPLE.")

;;; --- Metadata updaters (called from ewd.el) ---

(defun ewd-debug--update-tab-url (tab-id url)
  "Update URL for TAB-ID."
  (when (and tab-id url)
    (puthash (format "%s" tab-id) url ewd--tab-urls)))

(defun ewd-debug--update-tab-title (tab-id title)
  "Update title for TAB-ID."
  (when (and tab-id title)
    (puthash (format "%s" tab-id) title ewd--tab-titles)))

(defun ewd-debug--remove-tab (tab-id)
  "Remove TAB-ID from maps."
  (when tab-id
    (let ((tid (format "%s" tab-id)))
      (remhash tid ewd--tab-urls)
      (remhash tid ewd--tab-titles))))

;;; --- Helpers ---

(defun ewd--url-host (url)
  "Extract host from URL. For non-URLs, return as-is."
  (when (and url (stringp url))
    (let ((parsed (url-generic-parse-url url)))
      (or (url-host parsed) url))))

(defun ewd-debug--format-header (tab-id)
  "Format the org header string for TAB-ID."
  (if (eq tab-id 'ipc)
      "* EWD Debug Log [[ipc][ipc]]"
    (let* ((tid (format "%s" tab-id))
           (title (gethash tid ewd--tab-titles))
           (url (gethash tid ewd--tab-urls))
           (host (if url (ewd--url-host url) "unknown"))
           (display (if (and title (not (string-empty-p (string-trim title))))
                        title
                      host)))
      (format "* %s [[%s][link]] [[%s][uuid]]" display (or url "") tid))))

(defun ewd-debug--tab-key (tab-id)
  "Return hash key for TAB-ID."
  (if (eq tab-id 'ipc) "ipc" (format "%s" tab-id)))

;;; --- Public entry ---

(defun ewd-debug-log (tab-id line)
  "Queue LINE for TAB-ID; flushed into the buffer when Emacs is idle.
TAB-ID is a string (the UUID) or the symbol `ipc'."
  (push (cons tab-id line) ewd--debug-pending)
  (ewd-debug--schedule-flush))

(defun ewd-debug--schedule-flush ()
  "Schedule a deferred flush if one is not already scheduled."
  (unless ewd--debug-flush-timer
    (setq ewd--debug-flush-timer
          (run-with-idle-timer ewd--debug-flush-delay nil
                               #'ewd-debug--flush))))

;;; --- Section management via markers ---

(defun ewd-debug--ensure-section (tab-id)
  "Ensure the section for TAB-ID exists, creating it if needed.
For a tab the main `ipc' header is ensured first so it stays on top."
  (let ((key (ewd-debug--tab-key tab-id)))
    (unless (gethash key ewd--debug-insert-markers)
      (if (eq tab-id 'ipc)
          (ewd-debug--create-ipc-section)
        (ewd-debug--ensure-section 'ipc)
        (ewd-debug--create-tab-section tab-id)))))

(defun ewd-debug--create-ipc-section ()
  "Create the main ipc section at the top of the buffer."
  (goto-char (point-min))
  (let ((heading-pos (point)))
    (insert (ewd-debug--format-header 'ipc) "\n")
    (insert "#+BEGIN_EXAMPLE\n")
    (let ((insert-pos (point)))
      (insert "#+END_EXAMPLE\n")
      (puthash "ipc" (copy-marker heading-pos nil) ewd--debug-heading-markers)
      (puthash "ipc" (copy-marker insert-pos t) ewd--debug-insert-markers))))

(defun ewd-debug--create-tab-section (tab-id)
  "Create a section for TAB-ID at the end of the buffer."
  (goto-char (point-max))
  (unless (bolp) (insert "\n"))
  (insert "\n")
  (let ((heading-pos (point)))
    (insert (ewd-debug--format-header tab-id) "\n")
    (insert "#+BEGIN_EXAMPLE\n")
    (let ((insert-pos (point))
          (key (ewd-debug--tab-key tab-id)))
      (insert "#+END_EXAMPLE\n")
      (puthash key (copy-marker heading-pos nil) ewd--debug-heading-markers)
      (puthash key (copy-marker insert-pos t) ewd--debug-insert-markers))))

(defun ewd-debug--insert-line (tab-id line)
  "Insert LINE into the section for TAB-ID using its insertion marker."
  (let* ((key (ewd-debug--tab-key tab-id))
         (marker (gethash key ewd--debug-insert-markers)))
    (when (marker-position marker)
      (goto-char marker)
      (insert (format "[%s] %s\n"
                      (format-time-string "%H:%M:%S")
                      line)))))

(defun ewd-debug--refresh-heading (tab-id)
  "Update the heading for TAB-ID if its formatted text changed."
  (unless (eq tab-id 'ipc)
    (let* ((key (ewd-debug--tab-key tab-id))
           (marker (gethash key ewd--debug-heading-markers)))
      (when (marker-position marker)
        (goto-char marker)
        (let ((new (ewd-debug--format-header tab-id))
              (current (buffer-substring-no-properties
                        (line-beginning-position) (line-end-position))))
          (unless (string= new current)
            (delete-region (line-beginning-position) (line-end-position))
            (insert new)))))))

;;; --- State capture / restore ---

(defun ewd-debug--capture-state ()
  "Capture fold state plus cursor and scroll anchors as markers.
Return a plist :folds :cursor :windows.
:folds   - hash of section key -> non-nil when that section is folded.
:cursor  - marker at buffer point.
:windows - list of (WINDOW . START-MARKER) for windows showing the buffer.
The markers are released by `ewd-debug--restore-state'."
  (let ((folds (make-hash-table :test 'equal))
        (cursor (copy-marker (point) nil))
        (windows nil))
    (maphash
     (lambda (key marker)
       (when (marker-position marker)
         (save-excursion
           (goto-char marker)
           (let ((body-pos (1+ (line-end-position))))
             (puthash key
                      (and (< body-pos (point-max))
                           (get-char-property body-pos 'invisible))
                      folds)))))
     ewd--debug-heading-markers)
    (dolist (win (get-buffer-window-list (current-buffer) nil t))
      (push (cons win (copy-marker (window-start win) nil)) windows))
    (list :folds folds :cursor cursor :windows windows)))

(defun ewd-debug--restore-state (state)
  "Restore fold state, cursor and scroll from STATE, then release its markers.
Folding is applied first, cursor and scroll last."
  (let ((folds (plist-get state :folds))
        (cursor (plist-get state :cursor))
        (windows (plist-get state :windows)))
    (unwind-protect
        (progn
          ;; 1. Folds: refold what was folded; new sections fold too.
          (maphash
           (lambda (key marker)
             (when (marker-position marker)
               (save-excursion
                 (goto-char marker)
                 (let ((folded (gethash key folds 'new)))
                   (when (or (eq folded 'new) folded)
                     (outline-hide-subtree))))))
           ewd--debug-heading-markers)
          ;; 2. Cursor: the marker already travelled with the text.
          (when (marker-position cursor)
            (goto-char cursor))
          ;; 3. Scroll: each window back onto its own anchor line.
          (dolist (entry windows)
            (let ((win (car entry))
                  (start (cdr entry)))
              (when (and (window-live-p win)
                         (marker-position start)
                         (eq (window-buffer win) (current-buffer)))
                (set-window-start win start)))))
      (set-marker cursor nil)
      (dolist (entry windows)
        (set-marker (cdr entry) nil)))))

;;; --- Flush ---

(defun ewd-debug--flush ()
  "Flush pending records into the debug buffer."
  (setq ewd--debug-flush-timer nil)
  (when ewd--debug-pending
    (let ((batch (nreverse ewd--debug-pending))
          (buffer (get-buffer-create ewd--debug-buffer-name)))
      (setq ewd--debug-pending nil)
      (with-current-buffer buffer
        (unless (derived-mode-p 'org-mode)
          (org-mode))
        (let ((inhibit-read-only t)
              (state (ewd-debug--capture-state)))
          (unwind-protect
              (progn
                (outline-show-all)
                (dolist (rec batch)
                  (let ((tab-id (car rec))
                        (line (cdr rec)))
                    (ewd-debug--ensure-section tab-id)
                    (ewd-debug--refresh-heading tab-id)
                    (ewd-debug--insert-line tab-id line)))
                (font-lock-flush (point-min) (point-max))
                (org-element-cache-refresh (point-min)))
            (ewd-debug--restore-state state)))))))

;;; --- Cleanup ---

(defun ewd-debug--cleanup ()
  "Cleanup debug state."
  (when ewd--debug-flush-timer
    (cancel-timer ewd--debug-flush-timer)
    (setq ewd--debug-flush-timer nil))
  (setq ewd--debug-pending nil)
  (clrhash ewd--tab-urls)
  (clrhash ewd--tab-titles)
  (maphash (lambda (_k m) (set-marker m nil)) ewd--debug-heading-markers)
  (maphash (lambda (_k m) (set-marker m nil)) ewd--debug-insert-markers)
  (clrhash ewd--debug-heading-markers)
  (clrhash ewd--debug-insert-markers)
  (when (get-buffer ewd--debug-buffer-name)
    (kill-buffer ewd--debug-buffer-name)))

(provide 'ewd-debug)
;;; ewd-debug.el ends here
