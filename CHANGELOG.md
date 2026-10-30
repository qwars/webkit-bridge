# Changelog

Все значимые изменения в этом проекте будут документированы в этом файле.

Формат основан на [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
и этот проект придерживается [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-10-29

### Добавлено

#### Push-уведомления
- Notification `tabs_changed`: демон теперь отправляет JSON-RPC уведомления (без поля `id`) при любом изменении состояния вкладок
- Клиенты могут реагировать на уведомления, вызывая `list` для получения актуального состояния
- Push-модель заменяет polling, снижая нагрузку на сокет и улучшая отзывчивость

#### Управление видимостью табов (show/hide)
- Новые RPC-методы `show` и `hide` для переключения видимости существующих табов
- Табы могут динамически переключаться между headless и видимым состоянием
- Полезно для решения CAPTCHA и работы с интерактивными страницами
- Новое поле `visible` (boolean) в ответе `list` — статус видимости каждого таба
- Новое событие `TabVisibilityChanged { id, visible }` в системе событий
- Новые CLI-команды: `wkctl show --id <uuid>` и `wkctl hide --id <uuid>`
- Новые Emacs-команды: `ewd-show-tab`, `ewd-hide-tab`
- Новые глобальные биндинги: `C-c w S` (show), `C-c w H` (hide)
- Новые биндинги в буфере `*ewd-tabs*`: `s` (show), `h` (hide)

#### Улучшения буфера *ewd-tabs*
- Метки `[V]` (visible) и `[H]` (headless) для каждого таба
- Цветовая подсветка: видимые табы Emacs выделяются цветом (`font-lock-type-face`), чужие табы — серым (`font-lock-comment-face`)
- Сохранение позиции курсора при перерисовке буфера (работает с любой строки блока таба)
- Команды `k`, `o`, `RET`, `s`, `h` работают с любой строки блока вкладки (UUID, URL или Owner)

#### Переменные окружения
- `EWD_SOCKET_PATH`: новая переменная окружения для переопределения пути к сокету
- Приоритет: `EWD_SOCKET_PATH` > `$XDG_RUNTIME_DIR/webkit-bridge.sock` > ошибка

#### Интеграционные тесты
- Полный набор bash-интеграционных тестов (`bash/integration.sh`)
- 6 тестовых наборов: `basic`, `eval`, `errors`, `concurrent`, `orphan`, `shutdown`
- Изоляция через `EWD_SOCKET_PATH` — каждый тест использует свой сокет
- 33 интеграционных теста + 42 unit-теста

### Изменено

#### Breaking Changes: упрощение API
- Убраны префиксы `tab.*` из всех JSON-RPC методов:
  - `tab.create` → `create`
  - `tab.close` → `close`
  - `tab.navigate` → `navigate`
  - `tab.reload` → `reload`
  - `tab.list` → `list`
  - `tab.eval` → `eval`
  - `daemon.shutdown` → `shutdown`
- Более чистый и интуитивный API

#### Breaking Changes: путь к сокету
- Путь по умолчанию изменён: `/tmp/webkit-bridge.sock` → `$XDG_RUNTIME_DIR/webkit-bridge.sock`
- Более соответствует XDG Base Directory Specification
- Предотвращает конфликты между пользователями в многопользовательских системах

#### Breaking Changes: CLI-клиент (wkctl)
- Обновлён синтаксис команд с явными флагами:
  - `wkctl create https://example.com` → `wkctl create --url https://example.com`
  - `wkctl navigate <id> <url>` → `wkctl navigate --id <id> --url <url>`
  - `wkctl reload <id>` → `wkctl reload --id <id>`
  - `wkctl eval <id> <script>` → `wkctl eval --id <id> --script <script>`
  - `wkctl close <id>` → `wkctl close --id <id>`
- Явные флаги улучшают читаемость и предотвращают путаницу с порядком аргументов

### Улучшения
- Унификация завершения: демон корректно обрабатывает как `Ctrl+C`, так и `shutdown` через IPC
- Гарантированное удаление файла сокета при любом сценарии завершения
- Обработка `tabs_changed` в `wkctl`: клиент корректно игнорирует notifications и ждёт реальный ответ на запрос
- Корректная обработка `:json-false` в Emacs-клиенте (Emacs Lisp считает символы truthy)

### Исправлено
- Race condition при параллельных клиентах: `wkctl` теперь корректно обрабатывает notifications, приходящие вместе с ответами
- Утечка ресурсов при краше клиента: orphan cleanup корректно закрывает вкладки через 60 секунд
- Дублирование событий `TabClosed`: убраны лишние события в GTK-потоке
- Потеря unit-тестов: восстановлены все unit-тесты
- Паника `RefCell already borrowed` при `hide`: отсоединение WebView без вызова `close()` предотвращает рекурсивное заимствование
- Прыжок курсора в начало буфера при перерисовке: надёжный поиск UUID через `re-search-backward`

### Миграция

#### Для пользователей CLI
Обновите синтаксис команд:
```bash
# Старый:
wkctl create https://example.com
# Новый:
wkctl create --url https://example.com

# Старый:
wkctl eval abc123 'document.title'
# Новый:
wkctl eval --id abc123 --script 'document.title'

# Новые команды:
wkctl show --id <uuid>   # Показать таб в GTK-окне
wkctl hide --id <uuid>   # Скрыть таб (перевести в headless)
```

#### Для пользователей API
Обновите имена методов в JSON-RPC запросах:
```json
// Старый:
{"method": "tab.create", "params": {"url": "..."}}
// Новый:
{"method": "create", "params": {"url": "..."}}

// Новые методы:
{"method": "show", "params": {"id": "uuid"}}
{"method": "hide", "params": {"id": "uuid"}}
```

#### Для пользователей демона
Если вы полагались на `/tmp/webkit-bridge.sock`, установите кастомный путь:
```bash
# Опция 1: Использовать дефолтный XDG путь
./ewd-daemon

# Опция 2: Установить кастомный путь
EWD_SOCKET_PATH=/tmp/webkit-bridge.sock ./ewd-daemon
```

## [0.1.0] - 2026-10-28

### Добавлено
- Первый релиз
- Демон с интеграцией WebKitGTK
- JSON-RPC 2.0 протокол over Unix socket
- CLI-клиент (`wkctl`)
- Управление вкладками (создание, закрытие, навигация, перезагрузка, список)
- Выполнение JavaScript
- Поддержка нескольких клиентов с контролем доступа на основе владения
- Автоматическая очистка осиротевших вкладок (интервал 60 секунд)
- Graceful shutdown через IPC
- Unit-тесты для core-компонентов
