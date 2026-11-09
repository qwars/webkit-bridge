;;; ewd-translate.el --- Translation via ewd and translate.ru  -*- lexical-binding: t; -*-

;; Version: 0.1.0
;; Package-Requires: ((emacs "28.1") (ewd "0.8.0"))

(require 'ewd)
(require 'json)
(require 'cl-lib)
(require 'thingatpt)

(defgroup ewd-translate nil
  "Translation via ewd and translate.ru."
  :group 'ewd
  :prefix "ewd-translate-")

;;; --- Configuration ---

(defcustom ewd-translate-max-chunk-length 4000
  "Maximum length of a text chunk for translation."
  :type 'integer
  :group 'ewd-translate)

(defcustom ewd-translate-url "https://www.translate.ru/перевод"
  "URL of the dedicated translation tab."
  :type 'string
  :group 'ewd-translate)

(defcustom ewd-translate-js ""
  "JavaScript code or path to JS file to inject.
If empty, uses built-in `ewd-translate--default-js'."
  :type 'string
  :group 'ewd-translate)

(defcustom ewd-translate-page-load-timeout 30
  "Seconds to wait for the translation page to finish loading."
  :type 'integer
  :group 'ewd-translate)

;;; --- JS injection ---

(defvar ewd-translate--inject-js nil
  "Resolved JS code to inject.")

(defconst ewd-translate--default-js
  "(function() {
  if (window.__ewd_translator_ready) return;
  window.__ewd_translator_ready = true;

  function parseDictView(html) {
    const div = document.createElement('div');
    div.innerHTML = html;
    const lines = [];
    div.querySelectorAll('.cforms_result').forEach((form, i, arr) => {
      const w = (form.querySelector('.source_only') || {}).textContent || '';
      const t = (form.querySelector('.transcription') || {}).textContent || '';
      const p = (form.querySelector('.ref_psp') || {}).textContent || '';
      const f = (form.querySelector('.otherImportantForms') || {}).textContent || '';
      lines.push('')
      lines.push(w.trim() + ' ' + t.trim());
      lines.push('    ' + f.trim().replace(/(\\s|\\n)+/g, ' '));

      lines.push('');
      form.querySelectorAll('.translation-item').forEach(item => {
        const tr = (item.querySelector('.result_only') || {}).textContent || '';
        const ri = (item.querySelector('.ref_info') || {}).textContent || '';
        let l = '  ' + tr.trim();
        if (ri) l += ' ' + ri.trim();
        lines.push(l);
        item.querySelectorAll('.m-ex').forEach(ex => {
          const s = (ex.querySelector('.samSource') || {}).textContent || '';
          const tt = (ex.querySelector('.samTranslation') || {}).textContent || '';
          if (s) lines.push('    › ' + s.trim());
          if (tt) lines.push('    › ' + tt.trim());
        });
        lines.push('');
      });
      if (i < arr.length - 1) { lines.push('─'.repeat(80)); lines.push(''); }
    });
    return lines.join('\\n').trim();
  }

  var origDone = window.trru.translation.getTranslationDone;
  window.trru.translation.getTranslationDone = function(n, t) {
    try {
      if (n && n.error) window.ewd.send('translate_result', '*ERROR* ' + n.error);
      else if (n && n.dictHtml) window.ewd.send('translate_result', parseDictView(n.dictHtml));
      else if (n && n.text) window.ewd.send('translate_result', n.text);
    } catch (e) { console.error('EWD send error:', e); }
    if (origDone) origDone.call(this, n, t);
  };

  window.ewdTranslate = function(text) {
    try {
      if (window.trru && window.trru.api && typeof window.trru.api.onUIEvent === 'function')
        window.trru.api.onUIEvent(null);
    } catch(e) {}
    const sText = document.getElementById('sText');
    if (sText) {
      sText.value = text;
      setTimeout(() => {
        const btn = document.getElementById('btnTranslate');
        if (btn) btn.click();
      }, 168);
    }
  };

  setTimeout(() => {
    if (window.ewd && typeof window.ewd.send === 'function')
      window.ewd.send('translate_ready', { status: 'ok' });
  }, 100);
})();"
  "Default JS: parseDictView + permanent getTranslationDone hook + ewdTranslate.")

(defun ewd-translate--resolve-js ()
  "Compute `ewd-translate--inject-js' from `ewd-translate-js'."
  (setq ewd-translate--inject-js
        (cond
         ((string-empty-p ewd-translate-js) ewd-translate--default-js)
         ((file-exists-p ewd-translate-js)
          (with-temp-buffer (insert-file-contents ewd-translate-js) (buffer-string)))
         (t ewd-translate-js))))

;;; --- Buffer ---

(defvar ewd-translate--buffer-name "*EWD Translate*")

;;; --- State ---

(defvar ewd-translate--tab-id nil)
(defvar ewd-translate--js-initialized nil)
(defvar ewd-translate--handlers-registered nil)
(defvar ewd-translate--pending-operations nil)
(defvar ewd-translate--results nil)
(defvar ewd-translate--pending-chunks nil)
(defvar ewd-translate--current-request-index 0)

;;; --- Faces ---

(defface ewd-translate-header-face
  '((t :inherit font-lock-function-name-face :weight bold))
  "Session header (=== ... ===)."
  :group 'ewd-translate)

(defface ewd-translate-form-face
  '((t :inherit font-lock-keyword-face :weight bold))
  "Dictionary form header (word + transcription)."
  :group 'ewd-translate)

(defface ewd-translate-translation-face
  '((t :inherit default))
  "Translation line."
  :group 'ewd-translate)

(defface ewd-translate-ref-face
  '((t :inherit font-lock-comment-face :slant italic))
  "Reference info (part of speech, usage)."
  :group 'ewd-translate)

(defface ewd-translate-example-face
  '((t :inherit font-lock-string-face))
  "Example lines (source and translation)."
  :group 'ewd-translate)

(defface ewd-translate-separator-face
  '((t :inherit shadow))
  "Separator line (────)."
  :group 'ewd-translate)

(defface ewd-translate-error-face
  '((t :inherit error))
  "Error line."
  :group 'ewd-translate)

(defface ewd-translate-pending-face
  '((t :inherit shadow :slant italic))
  "Pending chunk placeholder."
  :group 'ewd-translate)

;;; --- Handlers ---

(defun ewd-translate--register-handlers ()
  "Register JS message handlers."
  (ewd-register-js-handler "translate_ready"
    (lambda (tab-id _data)
      (when (string= tab-id ewd-translate--tab-id)
        (setq ewd-translate--js-initialized t)
        (message "EWD Translate: ready")
        (ewd-translate--execute-pending-operations))))
  (ewd-register-js-handler "translate_result"
    (lambda (tab-id data)
      (when (and (string= tab-id ewd-translate--tab-id)
                 (> ewd-translate--current-request-index 0))
        (aset ewd-translate--results ewd-translate--current-request-index data)
        (ewd-translate--update-buffer (ewd-translate--render-results))
        (ewd-translate--send-next-chunk))))
  (setq ewd-translate--handlers-registered t))

(defun ewd-translate--execute-pending-operations ()
  "Execute queued operations."
  (dolist (op (prog1 ewd-translate--pending-operations
                (setq ewd-translate--pending-operations nil)))
    (ignore-errors (funcall op))))

;;; --- Background init ---

(defun ewd-translate--wait-and-inject (tab-id)
  "Wait for page readiness in TAB-ID, then inject JS."
  (let ((deadline (+ (float-time) ewd-translate-page-load-timeout))
        timer done)
    (setq timer (run-with-timer 0 0.2
      (lambda ()
        (cond
         (done nil)
         ((> (float-time) deadline)
          (setq done t) (cancel-timer timer)
          (message "EWD Translate: timeout"))
         (t
          (ewd-eval-async tab-id
            "!!window.trru && !!document.getElementById('sText') && !!(window.ewd && typeof window.ewd.send === 'function')"
            (lambda (msg)
              (unless done
                (when (string= (ewd--extract-eval-result msg) "true")
                  (setq done t) (cancel-timer timer)
                  (ewd-eval-async tab-id ewd-translate--inject-js
                    (lambda (_m) (message "EWD Translate: JS injected"))
                    (lambda (e) (message "EWD Translate: inject error: %s" e))))))
            (lambda (_e) nil)))))))))

(defun ewd-translate--init-tab ()
  "Find or create translation tab and prepare it."
  (message "EWD Translate: preparing tab...")
  (ewd-translate--resolve-js)
  (ewd-list-tabs-async
    (lambda (msg)
      (let* ((tabs (ewd--extract-list-tabs msg))
             (found (cl-find ewd-translate-url tabs
                             :key (lambda (tab) (cdr (assq 'url tab)))
                             :test #'string=)))
        (if found
            (progn (setq ewd-translate--tab-id (cdr (assq 'id found)))
                   (ewd-translate--wait-and-inject ewd-translate--tab-id))
          (ewd-create-tab-async ewd-translate-url
            (lambda (m) (setq ewd-translate--tab-id (ewd--extract-create-id m))
              (ewd-translate--wait-and-inject ewd-translate--tab-id))
            (lambda (e) (message "EWD Translate: create error: %s" e))))))
    (lambda (e) (message "EWD Translate: list error: %s" e))))

(defun ewd-translate--on-connected ()
  "Advice for `ewd-connect': init tab after connection."
  (advice-remove 'ewd-connect #'ewd-translate--on-connected)
  (ewd-translate--init-tab))

(defun ewd-translate--auto-start ()
  "Startup hook: connect to ewd and init translation tab."
  (unless ewd-translate--handlers-registered (ewd-translate--register-handlers))
  (if (ewd--connected-p)
      (ewd-translate--init-tab)
    (advice-add 'ewd-connect :after #'ewd-translate--on-connected)
    (ewd-connect)))

(add-hook 'emacs-startup-hook #'ewd-translate--auto-start)

;;; --- Text splitting ---

(defun ewd-translate--split-text (text)
  "Split TEXT into chunks by words.
Normalize whitespace: multiple spaces/tabs become single space,
preserve newlines. Fix word hyphenation across lines."
  (let* ((max-len ewd-translate-max-chunk-length)
         (chunks nil)
         (current "")
         (fixed (ewd-translate--normalize-hyphenation text))
         (tokens (ewd-translate--tokenize-and-normalize fixed)))
    (dolist (token tokens)
      (let* ((token-str token)
             (test (concat current token-str)))
        (if (<= (length test) max-len)
            (setq current test)
          (if (string-empty-p current)
              (progn
                (push token-str chunks)
                (setq current ""))
            (push current chunks)
            (setq current token-str)))))
    (unless (string-empty-p current)
      (push current chunks))
    (nreverse chunks)))

(defun ewd-translate--normalize-hyphenation (text)
  "Remove soft hyphens at line breaks in TEXT.
Joins words broken across lines by U+2010 (‑) or regular hyphen."
  (replace-regexp-in-string
   "[-\u2010][ \t]*\n[ \t]*\\([a-zа-я]\\)"
   "\\1"
   text))

(defun ewd-translate--tokenize-and-normalize (text)
  "Tokenize TEXT into words and whitespace.
Normalize whitespace: multiple spaces/tabs become single space,
preserve newlines."
  (let ((result nil)
        (pos 0)
        (len (length text)))
    (while (< pos len)
      ;; Collect whitespace
      (let ((ws-start pos))
        (while (and (< pos len)
                    (memq (aref text pos) '(?\s ?\t ?\n)))
          (setq pos (1+ pos)))
        (when (> pos ws-start)
          (let ((whitespace (substring text ws-start pos)))
            ;; Normalize: if contains newline, preserve; else single space
            (let ((normalized
                   (if (string-match "\n" whitespace)
                       whitespace
                     " ")))
              (push normalized result)))))
      ;; Collect word
      (let ((word-start pos))
        (while (and (< pos len)
                    (not (memq (aref text pos) '(?\s ?\t ?\n))))
          (setq pos (1+ pos)))
        (when (> pos word-start)
          (let ((word (substring text word-start pos)))
            (push word result)))))
    (nreverse result)))

;;; --- Rendering ---

(defun ewd-translate--render-results ()
  "Render results vector as display string."
  (let ((label (aref ewd-translate--results 0))
        (parts nil))
    (dotimes (i (1- (length ewd-translate--results)))
      (let ((r (aref ewd-translate--results (1+ i))))
        (push (cond
               ((null r) "[ожидает...]\n")
               ((string-prefix-p "*ERROR*" r) (concat r "\n"))
               (t (concat r "\n")))
              parts)))
    (concat (format "=== %s ===\n" label)
            (mapconcat #'identity (nreverse parts) ""))))

(defun ewd-translate--fontify-buffer ()
  "Apply semantic faces to current buffer based on strict line patterns."
  (let ((inhibit-read-only t))
    (save-excursion
      ;; 1. Session header
      (goto-char (point-min))
      (while (re-search-forward "^\\(===.*===\\)$" nil t)
        (put-text-property (match-beginning 1) (match-end 1)
                           'face 'ewd-translate-header-face))

      ;; 2. Error
      (goto-char (point-min))
      (while (re-search-forward "^\\(\\*ERROR\\*.*\\)$" nil t)
        (put-text-property (match-beginning 1) (match-end 1)
                           'face 'ewd-translate-error-face))

      ;; 3. Pending placeholder
      (goto-char (point-min))
      (while (re-search-forward "^\\(\\[ожидает\\.\\.\\.\\]\\)$" nil t)
        (put-text-property (match-beginning 1) (match-end 1)
                           'face 'ewd-translate-pending-face))

      ;; 4. Separator
      (goto-char (point-min))
      (while (re-search-forward "^\\(─+\\)$" nil t)
        (put-text-property (match-beginning 1) (match-end 1)
                           'face 'ewd-translate-separator-face))

      ;; 5. Example line (4 spaces + ›)
      (goto-char (point-min))
      (while (re-search-forward "^\\(    › .*\\)$" nil t)
        (put-text-property (match-beginning 1) (match-end 1)
                           'face 'ewd-translate-example-face))

      ;; 6. Translation line (exactly 2 spaces + text)
      (goto-char (point-min))
      (while (re-search-forward "^\\(  [^ \n].*\\)$" nil t)
        (put-text-property (match-beginning 1) (match-end 1)
                           'face 'ewd-translate-translation-face))

      ;; 7. Dictionary header (Single word, optionally followed by [transcription])
      ;; This strictly matches "cookie" or "Evolution [ˌevəˈluːʃn]"
      ;; It will NOT match regular sentences like "This is a test."
      (goto-char (point-min))
      (while (re-search-forward "^\\([A-Za-zА-Яа-я0-9][A-Za-zА-Яа-я0-9\\-']*\\(?:\\s-+\\[.*\\]\\)?\\)$" nil t)
        (let ((beg (match-beginning 1))
              (end (match-end 1)))
          ;; Apply only if not already colored (e.g., not a session header)
          (when (null (get-text-property beg 'face))
            (put-text-property beg end 'face 'ewd-translate-form-face)))))))

(defun ewd-translate--update-buffer (text)
  "Show TEXT in translation buffer with fontification."
  (with-current-buffer (get-buffer-create ewd-translate--buffer-name)
    (unless (derived-mode-p 'special-mode) (special-mode))
    (let ((inhibit-read-only t))
      (erase-buffer)
      (insert text)
      (ewd-translate--fontify-buffer)))
  (display-buffer (get-buffer ewd-translate--buffer-name)))

;;; --- Chunk processing ---

(defun ewd-translate--send-next-chunk ()
  "Send next chunk for translation."
  (let ((next (1+ ewd-translate--current-request-index))
        (total (1- (length ewd-translate--pending-chunks))))
    (message "EWD Translate: send-next-chunk: next=%d, total=%d, tab-id=%s"
             next total ewd-translate--tab-id)
    (when (<= next total)
      (setq ewd-translate--current-request-index next)
      (unless ewd-translate--tab-id
        (user-error "EWD Translate: tab-id is nil"))
      (message "EWD Translate: calling ewd-eval-async for chunk %d" next)
      (ewd-eval-async ewd-translate--tab-id
        (format "window.ewdTranslate(%s);"
                (json-encode-string (aref ewd-translate--pending-chunks next)))
        (lambda (_m) (message "EWD Translate: chunk %d sent successfully" next))
        (lambda (e) (message "EWD Translate: chunk %d error: %s" next e))))))

(defun ewd-translate--process-text (text label)
  "Process TEXT for translation with LABEL."
  (message "EWD Translate: process-text called, tab-id=%s, js-init=%s"
           ewd-translate--tab-id ewd-translate--js-initialized)
  (when (string-empty-p (string-trim text)) (user-error "Empty text"))
  (unless (ewd--connected-p) (user-error "EWD daemon not connected"))
  (unless ewd-translate--handlers-registered (ewd-translate--register-handlers))
  (if ewd-translate--js-initialized
      (progn
        (message "EWD Translate: JS ready, starting translation")
        (ewd-translate--start-translation text label))
    (message "EWD Translate: JS not ready, queueing operation")
    (ewd-translate--update-buffer (format "=== %s ===\n\n[Ожидание готовности...]\n" label))
    (push (lambda () (ewd-translate--start-translation text label))
          ewd-translate--pending-operations)))

(defun ewd-translate--start-translation (text label)
  "Start translating TEXT with LABEL."
  (let* ((chunks (ewd-translate--split-text text))
         (total (length chunks)))
    (message "EWD Translate: split into %d chunks" total)
    (when (= total 0)
      (user-error "No chunks to translate"))
    (setq ewd-translate--results (make-vector (1+ total) nil)
          ewd-translate--pending-chunks (make-vector (1+ total) nil)
          ewd-translate--current-request-index 0)
    (aset ewd-translate--results 0
          (format "%s / %d chunk%s" label total (if (= total 1) "" "s")))
    (dotimes (i total) (aset ewd-translate--pending-chunks (1+ i) (nth i chunks)))
    (ewd-translate--update-buffer (ewd-translate--render-results))
    (message "EWD Translate: sending first chunk")
    (ewd-translate--send-next-chunk)))

;;; --- Commands ---

(defun ewd-translate-region (start end)
  "Translate region from START to END."
  (interactive "r")
  (unless (use-region-p) (user-error "No region selected"))
  (ewd-translate--process-text
   (buffer-substring-no-properties start end)
   (format "Region (%d chars)" (- end start))))

(defun ewd-translate-word ()
  "Translate word at point."
  (interactive)
  (let ((word (thing-at-point 'word t)))
    (unless word (user-error "No word at point"))
    (let ((clean-word word))
      ;; Remove leading non-alphanumeric (except hyphen inside words)
      (setq clean-word (replace-regexp-in-string "^[^[:alnum:]-]+" "" clean-word))
      ;; Remove trailing non-alphanumeric
      (setq clean-word (replace-regexp-in-string "[^[:alnum:]-]+$" "" clean-word))
      (when (string-empty-p clean-word)
        (user-error "No translatable word at point"))
      (message "EWD Translate: raw='%s' clean='%s'" word clean-word)
      (ewd-translate--process-text clean-word (format "Word: %s" clean-word)))))

(defun ewd-translate-buffer ()
  "Translate entire buffer."
  (interactive)
  (ewd-translate--process-text
   (buffer-substring-no-properties (point-min) (point-max))
   (format "Buffer (%d chars)" (buffer-size))))

;;; --- Keybindings ---

(defvar ewd-translate-global-map
  (let ((m (make-sparse-keymap)))
    (define-key m (kbd "r") #'ewd-translate-region)
    (define-key m (kbd "w") #'ewd-translate-word)
    (define-key m (kbd "b") #'ewd-translate-buffer)
    m)
  "Keymap after `C-c t'.")

(define-key global-map (kbd "C-c t") ewd-translate-global-map)

(provide 'ewd-translate)
;;; ewd-translate.el ends here
