;;; ewd-deepseek.el --- DeepSeek chat integration for EWD -*- lexical-binding: t; -*-
;; Version: 0.0.3
;; Keywords: comm, tools

;;; Commentary:
;; Шаг 2: Создаем вкладку, логируем все события в debug буфер.
;; Используем событийную модель через ewd-network-event-hook.

;;; Code:

(require 'ewd)

(defcustom ewd-deepseek-url "https://chat.deepseek.com/"
  "URL страницы чата DeepSeek для открытия при запуске."
  :type 'string
  :group 'ewd)

(defcustom ewd-deepseek-debug-mode t
  "Включить режим отладки с логированием в отдельный буфер."
  :type 'boolean
  :group 'ewd)

(defvar ewd-deepseek-tab-id nil
  "ID текущей активной вкладки DeepSeek.")

(defvar ewd-deepseek-debug-buffer-name "*ewd-deepseek-debug*"
  "Имя буфера для debug логов.")

(defun ewd-deepseek--log (format-string &rest args)
  "Записать сообщение в debug буфер если ewd-deepseek-debug-mode включен.
FORMAT-STRING и ARGS - формат сообщения."
  (when ewd-deepseek-debug-mode
    (let ((buf (get-buffer-create ewd-deepseek-debug-buffer-name))
          (timestamp (format-time-string "%H:%M:%S")))
      (with-current-buffer buf
        (goto-char (point-max))
        (insert (format "[%s] %s\n" timestamp (apply #'format format-string args)))
        ;; Автоматически прокручивать вниз
        (when (get-buffer-window buf)
          (with-selected-window (get-buffer-window buf)
            (goto-char (point-max))))))))

(defun ewd-deepseek-show-debug-buffer ()
  "Показать debug буфер в другом окне."
  (interactive)
  (let ((buf (get-buffer-create ewd-deepseek-debug-buffer-name)))
    (with-current-buffer buf
      (special-mode)
      (goto-char (point-max)))
    (display-buffer buf)))

(defun ewd-deepseek--network-event-handler (tab-id event)
  "Обработчик сетевых событий для отладки.
TAB-ID - идентификатор вкладки, EVENT - данные события."
  ;; Логируем все события для нашей вкладки
  (when (string= tab-id ewd-deepseek-tab-id)
    (ewd-deepseek--log "NETWORK EVENT: %s" (prin1-to-string event))
    
    ;; Здесь можно добавить обработку конкретных событий
    ;; Например, когда демон отправит событие о загрузке страницы
    ;; (when (string= event "tab.loaded")
    ;;   (ewd-deepseek--log "Страница загружена!")
    ;;   )
    ))

(defun ewd-deepseek-create-tab ()
  "Создать вкладку с DeepSeek чатом и включить логирование."
  (interactive)
  (ewd-deepseek--log "=== EWD-DeepSeek: Начало инициализации ===")
  (ewd-deepseek--log "URL: %s" ewd-deepseek-url)
  (ewd-deepseek--log "Debug mode: %s" ewd-deepseek-debug-mode)
  
  (when (fboundp 'ewd-create-tab-async)
    (ewd-deepseek--log "Создание вкладки...")
    (ewd-create-tab-async
     ewd-deepseek-url
     (lambda (msg)
       (ewd-deepseek--log "Получен ответ от ewd-create-tab-async:")
       (ewd-deepseek--log "  %s" (prin1-to-string msg))
       
       (let* ((result (cdr (assq 'result msg)))
              (tab-id (cdr (assq 'id result))))
         (when tab-id
           (setq ewd-deepseek-tab-id tab-id)
           (ewd-deepseek--log "Вкладка создана, ID: %s" tab-id)
           
           ;; Регистрируем обработчик сетевых событий
           (add-hook 'ewd-network-event-hook #'ewd-deepseek--network-event-handler)
           (ewd-deepseek--log "Зарегистрирован обработчик ewd-network-event-hook")
           
           ;; Показываем debug буфер
           (when ewd-deepseek-debug-mode
             (ewd-deepseek-show-debug-buffer)))))
     (lambda (err)
       (ewd-deepseek--log "ОШИБКА создания вкладки: %s" err)))))

;; Автоматическое создание вкладки после завершения загрузки Emacs
(add-hook 'after-init-hook #'ewd-deepseek-create-tab)

(provide 'ewd-deepseek)
;;; ewd-deepseek.el ends here
