;;; ewd-translate.el --- Translation via ewd and translate.ru  -*- lexical-binding: t; -*-
;; Version: 0.40.0
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

(defcustom ewd-translate-max-chunk-length 5000
  "Maximum length of a text chunk for translation."
  :type 'integer
  :group 'ewd-translate)

(defcustom ewd-translate-url "https://www.translate.ru/перевод"
  "URL of the dedicated translation tab."
  :type 'string
  :group 'ewd-translate)

(defcustom ewd-translate-js ""
  "JavaScript code or path to JS file to inject into the translation tab.
If empty, uses built-in `ewd-translate--default-js'."
  :type 'string
  :group 'ewd-translate)

(defcustom ewd-translate-page-load-timeout 30
  "Seconds to wait for the translation page to finish loading."
  :type 'integer
  :group 'ewd-translate)

;;; --- JS injection ---

(defvar ewd-translate--inject-js ""
  "JavaScript code to inject into the translation tab.")

(defconst ewd-translate--ready-js
  "(function() {
    function checkReady() {
        if (window.ewd && typeof window.ewd.send === 'function') {
            try {
                window.ewd.send('translate_ready', { status: 'ok' });
            } catch(e) {
                setTimeout(checkReady, 100);
            }
        } else {
            setTimeout(checkReady, 100);
        }
    }
    checkReady();
})();"
  "JavaScript appended after `ewd-translate--inject-js'.
Polls for window.ewd.send and sends translate_ready when available.")

(defconst ewd-translate--default-js
  "(function() {
    if (window.__ewd_translator_ready) return;
    window.__ewd_translator_ready = true;

    const origXHRSend = XMLHttpRequest.prototype.send;

    function parseDictView(html) {
        const div = document.createElement('div');
        div.innerHTML = html;

        const lines = [];
        const forms = div.querySelectorAll('.cforms_result');

        forms.forEach((form, formIndex) => {
            const word = (form.querySelector('.source_only') || {}).textContent || '';
            const transcription = (form.querySelector('.transcription') || {}).textContent || '';
            const psp = (form.querySelector('.ref_psp') || {}).textContent || '';
            const importantForms = (form.querySelector('.otherImportantForms') || {}).textContent || '';

            lines.push(word.trim() + ' ' + transcription.trim());

            if (psp || importantForms) {
                let header = psp.trim();
                if (importantForms) {
                    header += (header ? ': ' : '') + importantForms.trim();
                }
                lines.push(header);
            }
            lines.push('');

            const items = form.querySelectorAll('.translation-item');
            items.forEach(item => {
                const translation = (item.querySelector('.result_only') || {}).textContent || '';
                const refInfo = (item.querySelector('.ref_info') || {}).textContent || '';

                let line = '  ' + translation.trim();
                if (refInfo) line += ' ' + refInfo.trim();
                lines.push(line);
                const examples = item.querySelectorAll('.m-ex');
                examples.forEach(ex => {
                    const src = (ex.querySelector('.samSource') || {}).textContent || '';
                    const tr = (ex.querySelector('.samTranslation') || {}).textContent || '';
                    if (src) lines.push('    › ' + src.trim());
                    if (tr) lines.push('    › ' + tr.trim());
                });
                lines.push('');
            });

            if (formIndex < forms.length - 1) {
                lines.push('─'.repeat(40));
                lines.push('');
            }
        });

        return lines.join('\\n').trim();
    }

    XMLHttpRequest.prototype.send = function(...args) {
        this.addEventListener('load', function() {
            try {
                const url = new URL(this.responseURL);
                if (!(url.host === location.host &&
                      (url.pathname.includes('getTranslation') || url.pathname.includes('getSelection')))) {
                    return;
                }
                if (this.status !== 200) {
                    throw new Error(`HTTP ${this.status}: ${this.statusText}`);
                }
                const json = JSON.parse(this.responseText);
                if (json.error) {
                    window.ewd.send('translate_result', `*ERROR* ${json.error}`);
                } else if (json.dictHtml) {
                    window.ewd.send('translate_result', parseDictView(json.dictHtml));
                } else if (json.text) {
                    window.ewd.send('translate_result', json.text);
                }
            } catch(e) {
                try {
                    const url = new URL(this.responseURL);
                    if (url.host === location.host &&
                        (url.pathname.includes('getTranslation') || url.pathname.includes('getSelection'))) {
                        window.ewd.send('translate_result', `*ERROR* ${e.message || e}`);
                    }
                } catch(_) {}
            }
        });
        return origXHRSend.apply(this, args);
    };

    // Разблокируем anti-bot механизм translate.ru.
    // Сайт ждёт первого доверенного события пользователя (isTrusted=true),
    // прежде чем разрешить вызовы API. Вызов onUIEvent(null) обходит эту проверку.
    function unlockSiteApi() {
        try {
            if (window.trru && window.trru.api && typeof window.trru.api.onUIEvent === 'function') {
                window.trru.api.onUIEvent(null);
            }
        } catch(e) {}
    }

    window.ewdTranslate = function(text) {
        unlockSiteApi();
        const sText = document.getElementById('sText');
        if (sText) {
            sText.value = text;
            setTimeout(() => {
                const btn = document.getElementById('btnTranslate');
                if (btn) btn.click();
            }, 168);
        }
    };
})();"
  "Built-in JavaScript code for translation with anti-bot bypass.")

;;; --- Buffer management ---

(defvar ewd-translate--buffer-name "*EWD Translate*"
  "Name of the translation output buffer.")

(defun ewd-translate--update-buffer (text)
  "Create or show *EWD Translate* buffer and replace all content with TEXT."
  (let ((buf (get-buffer-create ewd-translate--buffer-name)))
    (with-current-buffer buf
      (unless (derived-mode-p 'special-mode)
        (special-mode))
      (let ((inhibit-read-only t))
        (erase-buffer)
        (insert text)))
    (display-buffer buf)
    buf))

;;; --- State ---

(defvar ewd-translate--tab-id nil
  "ID of the dedicated translation tab.")

(defvar ewd-translate--js-initialized nil
  "Non-nil when translator JS is installed in the live document.")

(defvar ewd-translate--handlers-registered nil
  "Non-nil if JS message handlers have been registered.")

(defvar ewd-translate--injecting nil
  "Non-nil while waiting for page and injecting translator JS.")

;;; --- JS initialization ---

(defun ewd-translate--init-js ()
  "Initialize `ewd-translate--inject-js' from `ewd-translate-js'."
  (setq ewd-translate--inject-js
        (cond
         ((string-empty-p ewd-translate-js) ewd-translate--default-js)
         ((file-exists-p ewd-translate-js)
          (with-temp-buffer
            (insert-file-contents ewd-translate-js)
            (buffer-string)))
         (t ewd-translate-js))))

;;; --- Page readiness ---

(defun ewd-translate--wait-for-page-load (tab-id callback)
  "Wait until translate.ru input field and EWD bridge are available.
Then call CALLBACK with t.  On timeout call CALLBACK with nil."
  (let ((deadline (+ (float-time) ewd-translate-page-load-timeout))
        (timer nil)
        (done nil))
    (setq timer
          (run-with-timer
           0 0.2
           (lambda ()
             (cond
              (done nil)
              ((> (float-time) deadline)
               (setq done t)
               (cancel-timer timer)
               (funcall callback nil))
              (t
               (ewd-eval-async
                tab-id
                "!!document.getElementById('sText') && !!(window.ewd && typeof window.ewd.send === 'function')"
                (lambda (msg)
                  (unless done
                    (let ((ready (ewd--extract-eval-result msg)))
                      (when (string= ready "true")
                        (setq done t)
                        (cancel-timer timer)
                        (funcall callback t)))))
                (lambda (_err) nil)))))))))

;;; --- Connection check ---

(defun ewd-translate--check-connection ()
  "Check connection to ewd-daemon and ensure translation tab is set up."
  (condition-case err
      (progn
        (unless (ewd--connected-p)
          (ewd-connect))
        (unless ewd-translate--handlers-registered
          (ewd-translate--register-handlers))
        (unless ewd-translate--js-initialized
          (unless ewd-translate--tab-id
            (let* ((msg (ewd--sync-request #'ewd-list-tabs-async))
                   (tabs (ewd--extract-list-tabs msg))
                   (found (cl-find ewd-translate-url tabs
                                   :key (lambda (tab) (cdr (assq 'url tab)))
                                   :test #'string=)))
              (if found
                  (setq ewd-translate--tab-id (cdr (assq 'id found)))
                (let ((create-msg (ewd--sync-request
                                   #'ewd-create-tab-async
                                   ewd-translate-url)))
                  (setq ewd-translate--tab-id
                        (ewd--extract-create-id create-msg))))))
          (ewd-translate--init-js)
          (message "EWD Translate: waiting for page...")
          (let ((tab-id ewd-translate--tab-id))
            (ewd-translate--wait-for-page-load
             tab-id
             (lambda (ready)
               (when ready
                 (message "EWD Translate: page ready, injecting JS")
                 (ewd-eval-async
                  tab-id
                  (concat ewd-translate--inject-js "\n" ewd-translate--ready-js)
                  (lambda (_msg)
                    (setq ewd-translate--js-initialized t)
                    (message "EWD Translate: JS injected"))
                  (lambda (err)
                    (message "EWD Translate: eval ERROR: %s" err))))))))
        ewd-translate--js-initialized)
    (error (error-message-string err))))


;;; --- Text splitting ---

(defun ewd-translate--split-text (text)
  "Split TEXT into chunks of at most max length, by sentences."
  (let ((chunks nil)
        (current "")
        (max-len ewd-translate-max-chunk-length))
    (dolist (para (split-string text "\n\n+" t))
      (let ((p (string-trim para)))
        (unless (string-empty-p p)
          (if (<= (length (concat current "\n\n" p)) max-len)
              (setq current (if (string-empty-p current) p (concat current "\n\n" p)))
            (unless (string-empty-p current) (push current chunks))
            (if (<= (length p) max-len)
                (setq current p)
              (dolist (sent (split-string p "\\([.!?]+\\s-+\\)" t))
                (let ((s (string-trim sent)))
                  (unless (string-empty-p s)
                    (if (<= (length (concat current " " s)) max-len)
                        (setq current (if (string-empty-p current) s (concat current " " s)))
                      (unless (string-empty-p current) (push current chunks))
                      (setq current s))))))))))
    (unless (string-empty-p current) (push current chunks))
    (nreverse chunks)))

;;; --- Results accumulation ---

(defvar ewd-translate--results nil
  "Vector of translation results. Index 0 contains the label.")

(defvar ewd-translate--pending-chunks nil
  "Vector of original text chunks to translate.")

(defvar ewd-translate--current-request-index 0
  "Index of the current chunk being translated.")

;;; --- Rendering ---

(defface ewd-translate-error-face
  '((t :foreground "red"))
  "Face for translation errors."
  :group 'ewd-translate)

(defun ewd-translate--render-results ()
  "Render current state of `ewd-translate--results' as a string for the buffer."
  (let* ((label (aref ewd-translate--results 0))
         (total (1- (length ewd-translate--results)))
         (text (format "=== %s ===\n\n" label)))
    (dotimes (i total)
      (let ((result (aref ewd-translate--results (1+ i))))
        (setq text
              (concat text
                      (cond
                       ((null result) "[ожидает...]\n")
                       ((string-prefix-p "*ERROR*" result)
                        (let ((rest (substring result (length "*ERROR*"))))
                          (concat (propertize "*ERROR*" 'face 'ewd-translate-error-face)
                                  rest "\n\n")))
                       (t (concat result "\n\n")))))))
    text))

;;; --- Message handlers ---

(defun ewd-translate--on-ready (tab-id _data)
  "Handle translate_ready message from TAB-ID."
  (when (and ewd-translate--tab-id
             (string= tab-id ewd-translate--tab-id))
    (setq ewd-translate--js-initialized t
          ewd-translate--injecting nil)
    (message "EWD Translate: Tab ready")))

(defun ewd-translate--on-result (tab-id data)
  "Handle translate_result message from TAB-ID with DATA."
  (when (and ewd-translate--tab-id
             (string= tab-id ewd-translate--tab-id)
             (> ewd-translate--current-request-index 0))
    (aset ewd-translate--results ewd-translate--current-request-index data)
    (ewd-translate--update-buffer (ewd-translate--render-results))
    (ewd-translate--send-next-chunk)))

(defun ewd-translate--register-handlers ()
  "Register JS message handlers using ewd API."
  (ewd-register-js-handler "translate_ready" #'ewd-translate--on-ready)
  (ewd-register-js-handler "translate_result" #'ewd-translate--on-result)
  (setq ewd-translate--handlers-registered t))

;;; --- Chunk processing ---

(defun ewd-translate--send-next-chunk ()
  "Send the next chunk for translation."
  (let ((next-index (1+ ewd-translate--current-request-index))
        (total (1- (length ewd-translate--pending-chunks))))
    (when (<= next-index total)
      (setq ewd-translate--current-request-index next-index)
      (let ((chunk (aref ewd-translate--pending-chunks next-index)))
        (ewd-eval-async
         ewd-translate--tab-id
         (format "window.ewdTranslate(%s);" (json-encode-string chunk))
         (lambda (_msg) nil)
         (lambda (err)
           (message "EWD Translate: Send error for chunk %d: %s"
                    ewd-translate--current-request-index err)))))))

(defun ewd-translate--process-text (text label)
  "Process TEXT for translation with LABEL."
  (when (string-empty-p (string-trim text))
    (user-error "Empty text"))
  (ewd-translate--update-buffer
   (format "=== %s ===\n\n[Инициализация...]\n(C-g — прервать)\n" label))
  (let ((result (ewd-translate--check-connection)))
    (when (stringp result)
      (ewd-translate--update-buffer
       (format "=== %s ===\n\nОшибка: %s\n" label result))
      (user-error "Connection failed: %s" result))
    (unless ewd-translate--js-initialized
      (ewd-translate--update-buffer
       (format "=== %s ===\n\n[Инициализация вкладки...]\n(C-g — прервать)\n" label))
      (let ((deadline (+ (float-time) ewd-translate-page-load-timeout)))
        (while (and (not ewd-translate--js-initialized)
                    (< (float-time) deadline))
          (accept-process-output nil 0.1)
          (unless (buffer-live-p (get-buffer ewd-translate--buffer-name))
            (cl-return-from ewd-translate--process-text nil)))
        (unless ewd-translate--js-initialized
          (ewd-translate--update-buffer
           (format "=== %s ===\n\nОшибка: Timeout ожидания готовности вкладки\n" label))
          (user-error "EWD Translate: Timeout"))))
    (let* ((chunks (ewd-translate--split-text text))
           (total (length chunks))
           (full-label (format "%s / %d chunk%s"
                               label total (if (= total 1) "" "s"))))
      (setq ewd-translate--results (make-vector (1+ total) nil))
      (setq ewd-translate--pending-chunks (make-vector (1+ total) nil))
      (aset ewd-translate--results 0 full-label)
      (dotimes (i total)
        (aset ewd-translate--pending-chunks (1+ i) (nth i chunks)))
      (setq ewd-translate--current-request-index 0)
      (ewd-translate--update-buffer (ewd-translate--render-results))
      (ewd-translate--send-next-chunk))))

;;; --- Commands ---

(defun ewd-translate-region (start end)
  "Translate the selected region from START to END."
  (interactive "r")
  (unless (use-region-p)
    (user-error "No region selected"))
  (let* ((text (buffer-substring-no-properties start end))
         (label (format "Region (%d chars)" (length text))))
    (ewd-translate--process-text text label)))

(defun ewd-translate-word ()
  "Translate the word at point."
  (interactive)
  (let ((word (thing-at-point 'word t)))
    (unless word
      (user-error "No word at point"))
    (ewd-translate--process-text word (format "Word: %s" word))))

(defun ewd-translate-buffer ()
  "Translate the entire buffer."
  (interactive)
  (let* ((text (buffer-substring-no-properties (point-min) (point-max)))
         (label (format "Buffer (%d chars)" (length text))))
    (ewd-translate--process-text text label)))

;;; --- Keybindings ---

(defvar ewd-translate-global-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "r") #'ewd-translate-region)
    (define-key map (kbd "w") #'ewd-translate-word)
    (define-key map (kbd "b") #'ewd-translate-buffer)
    map)
  "Keymap for EWD Translate commands after `C-c t'.")

(define-key global-map (kbd "C-c t") ewd-translate-global-map)

(provide 'ewd-translate)
;;; ewd-translate.el ends here
