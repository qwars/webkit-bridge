# webkit-bridge

Высокопроизводительный, легковесный демон WebKitGTK для автоматизации браузера в headless и видимом режимах, управляемый через JSON-RPC 2.0 over Unix sockets.

## Обзор

`webkit-bridge` — это рабочее пространство Rust, предоставляющее демон (`ewd-daemon`), CLI-утилиту (`wkctl`), терминальный интерфейс (`ewd-tui`) и Emacs-клиент (`ewd.el`) для управления экземплярами браузера WebKitGTK через чистый JSON-RPC API. Предназначен для интеграции со скриптами, инструментами автоматизации и любыми клиентами, совместимыми с JSON-RPC 2.0.

## Ключевые особенности

- Изолированное управление вкладками с автоматической очисткой осиротевших вкладок
- Контроль доступа на основе клиентов — каждый клиент может управлять только созданными им вкладками
- Поддержка прокси (SOCKS5, HTTP) для анонимного просмотра
- **Headless и видимый режимы** с динамическим переключением через команды `show`/`hide`
- Push-уведомления через JSON-RPC notification `tabs_changed`
- Zero-copy Unix socket IPC для максимальной производительности
- Graceful shutdown через IPC-команду
- **Терминальный интерфейс** (`ewd-tui`) — управление проектами, авто-сборка с esbuild, инспектор сети
- **Интеграция с Emacs** — полнофункциональный клиент с управлением вкладками, выполнением JavaScript, дашбордом

## Архитектура

```
webkit-bridge/
├── ewd-core/          # Библиотечный крейт (переиспользуемый API)
├── ewd-daemon/        # Бинарный крейт (автономный демон)
├── wkctl/             # CLI-утилита для тестирования
├── ewd-tui/           # Терминальный интерфейс (ratatui) — управление проектами, инспектор сети
├── emacs/             # Emacs-клиент (ewd.el)
├── elisp-packages/    # Emacs Lisp пакеты:
│   ├── ewd/           #   Основной клиент EWD
│   ├── ewd-deepseek/  #   Интеграция с DeepSeek AI
│   └── ewd-translate/ #   Плагин перевода
├── bash/              # Интеграционные тесты
├── docs/              # Документация (протокол, события)
└── release-pkg/       # Скрипты для сборки релизных пакетов
```

- `ewd-core`: Общая библиотека с `WebViewManager`, `Tab`, `Profile` и системой событий
- `ewd-daemon`: Автономный демон с GTK main loop + Tokio async runtime
- `wkctl`: CLI-клиент для тестирования и скриптов
- `ewd-tui`: Терминальный интерфейс для управления проектами, авто-сборки с esbuild, инспектора сети
- `emacs/`: Emacs Lisp клиент для бесшовной интеграции
- `elisp-packages/`: Дополнительные Emacs Lisp пакеты (DeepSeek AI, перевод)
- `bash/`: Набор интеграционных тестов
- `docs/`: Спецификация протокола и справочник событий

## Системные требования

### Поддерживаемые платформы

- ОС: Linux (тестировалось на Debian 13, должно работать на любом современном дистрибутиве)
- Дисплей: Wayland или X11
- GPU: Любой GPU с подходящими драйверами (тестировалось на AMDGPU, Intel, NVIDIA)
- Терминал: поддержка 256 цветов (true color рекомендуется для `ewd-tui`)

### Необходимые зависимости

Системные пакеты (пример для Debian/Ubuntu):

```bash
sudo apt install \
  libwebkitgtk-6.0-dev \
  libgtk-4-dev \
  libsoup-3.0-dev \
  libjavascriptcoregtk-6.0-dev \
  pkg-config \
  build-essential
```

Для `ewd-tui`:

```bash
sudo apt install esbuild   # или: npm install -g esbuild
```

Rust toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Установка

### Сборка из исходников

```bash
git clone <repository-url>
cd webkit-bridge
cargo build --release
```

Бинарники будут в `target/release/`:
- `ewd-daemon` — Основной демон
- `wkctl` — CLI-клиент
- `ewd-tui` — Терминальный интерфейс

### Установка в систему (опционально)

```bash
sudo cp target/release/ewd-daemon /usr/local/bin/
sudo cp target/release/wkctl /usr/local/bin/
sudo cp target/release/ewd-tui /usr/local/bin/
```

### Сборка отдельных компонентов

```bash
# Только демон
cargo build --release -p ewd-daemon

# Только CLI-клиент
cargo build --release -p wkctl

# Только терминальный интерфейс
cargo build --release -p ewd-tui
```

## Запуск демона

### Ручной запуск

```bash
ewd-daemon
# или с прокси:
ewd-daemon --proxy socks5://127.0.0.1:9050
```

Демон выполнит:
1. Инициализацию GTK4 и WebKitGTK
2. Создание Unix сокета по пути `$XDG_RUNTIME_DIR/webkit-bridge.sock` (по умолчанию)
3. Ожидание JSON-RPC подключений
4. Логирование событий в stdout

### Systemd User Service (рекомендуется)

Создайте `~/.config/systemd/user/ewd-daemon.service`:

```ini
[Unit]
Description=WebKit Bridge Daemon
After=graphical-session.target

[Service]
Type=simple
ExecStart=/usr/local/bin/ewd-daemon
Environment="WEBKIT_DISABLE_COMPOSITING_MODE=1"
Environment="WEBKIT_DISABLE_DMABUF_RENDERER=1"
Environment="GDK_BACKEND=wayland"
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

Включите и запустите:

```bash
systemctl --user daemon-reload
systemctl --user enable ewd-daemon
systemctl --user start ewd-daemon
```

Проверка статуса:

```bash
systemctl --user status ewd-daemon
journalctl --user -u ewd-daemon -f
```

## Переменные окружения

| Переменная                       | Описание                                                              |
|----------------------------------|-----------------------------------------------------------------------|
| `EWD_SOCKET_PATH`                | Пользовательский путь к сокету (переопределяет значение по умолчанию) |
| `XDG_RUNTIME_DIR`                | Стандартная XDG-директория (используется, если `EWD_SOCKET_PATH` не задан) |
| `WEBKIT_DISABLE_COMPOSITING_MODE`| Рекомендуется для стабильности Wayland/AMDGPU                         |
| `WEBKIT_DISABLE_DMABUF_RENDERER` | Рекомендуется для стабильности Wayland/AMDGPU                         |
| `GDK_BACKEND`                    | Рекомендуется: `wayland`                                              |
| `EWD_DAEMON_PATH`                | Пользовательский путь к бинарнику `ewd-daemon` (используется `ewd-tui`) |

Приоритет пути к сокету:
1. `EWD_SOCKET_PATH` (если задан)
2. `$XDG_RUNTIME_DIR/webkit-bridge.sock`
3. Ошибка, если ни один не задан

## Справочник JSON-RPC API

Демон общается через JSON-RPC 2.0 по Unix domain sockets в формате Newline-Delimited JSON (NDJSON).

Путь к сокету: `$XDG_RUNTIME_DIR/webkit-bridge.sock` (по умолчанию)

### Идентификация клиента

Каждое подключение получает уникальный `client_id` (UUID). Клиенты могут переопределять его, указывая `"client_id"` в параметрах запроса (используется `wkctl` с фиксированным ID `00000000-0000-0000-0000-000000000000`).

### Методы

#### `create`

Создание новой вкладки браузера.

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "create",
  "params": {
    "url": "https://example.com",
    "visible": true,
    "client_id": "uuid-string"
  },
  "id": 1
}
```

**Ответ:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9"
  },
  "id": 1
}
```

Параметры:
- `url` (string, опционально): URL для загрузки. По умолчанию: `"about:blank"`
- `visible` (boolean, опционально): Показать GTK-окно. По умолчанию: `false`
- `client_id` (string, опционально): Переопределение UUID клиента

#### `close`

Закрытие существующей вкладки. Только владелец может закрыть свою вкладку.

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "close",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "client_id": "uuid-string"
  },
  "id": 2
}
```

**Ответ (успех):**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 2
}
```

**Ответ (ошибка — не владелец):**
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32000,
    "message": "Not owner of tab: ac93f48c-9b3b-4b64-8852-966d2af207b9"
  },
  "id": 2
}
```

#### `navigate`

Навигация вкладки на новый URL. Только владелец может управлять навигацией.

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "navigate",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "url": "https://example.org",
    "client_id": "uuid-string"
  },
  "id": 3
}
```

**Ответ:**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 3
}
```

#### `reload`

Перезагрузка вкладки. Только владелец может перезагружать свои вкладки.

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "reload",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "client_id": "uuid-string"
  },
  "id": 4
}
```

**Ответ:**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 4
}
```

#### `eval`

Выполнение JavaScript в контексте вкладки. Только владелец может выполнять JS.

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "eval",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "script": "document.title",
    "client_id": "uuid-string"
  },
  "id": 5
}
```

**Ответ:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "result": "\"Example Domain\""
  },
  "id": 5
}
```

#### `list`

Список всех открытых вкладок. Доступно всем клиентам (без проверки владения).

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "list",
  "params": {
    "client_id": "uuid-string"
  },
  "id": 6
}
```

**Ответ:**
```json
{
   "jsonrpc": "2.0",
   "result": [
     {
       "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
       "url": "https://example.com",
       "title": "Example Domain",
       "owner": "00000000",
       "visible": false
     },
     {
       "id": "bb91aed9-1d07-46ef-87ae-f4d75689881c",
       "url": "https://google.com",
       "title": "Google",
       "owner": "eff7e05a",
       "visible": true
     }
   ],
   "id": 6
 }
```

Поля:
- `id` (string): UUID вкладки
- `url` (string): Текущий URL
- `title` (string): Заголовок страницы
- `owner` (string): Короткий UUID (первые 8 символов) владельца
- `visible` (boolean): Видимость вкладки в GTK-окне

#### `show`

Показать вкладку в GTK-окне. Если вкладка уже видима, окно получает фокус. Только владелец может показать свою вкладку.

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "show",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "client_id": "uuid-string"
  },
  "id": 8
}
```

**Ответ:**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 8
}
```

Сценарий использования: решение CAPTCHA вручную или взаимодействие со страницами, требующими ввода пользователя.

#### `hide`

Скрыть вкладку (перевести в headless-режим). GTK-окно закрывается, но вкладка и её WebView остаются активными. Только владелец может скрыть свою вкладку.

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "hide",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "client_id": "uuid-string"
  },
  "id": 9
}
```

**Ответ:**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 9
}
```

Сценарий использования: после решения CAPTCHA скрыть окно и вернуться в headless-режим.

#### `shutdown`

Корректное завершение работы демона.

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "shutdown",
  "params": {
    "client_id": "uuid-string"
  },
  "id": 7
}
```

**Ответ:**
```json
{
  "jsonrpc": "2.0",
  "result": null,
  "id": 7
}
```

### Уведомления

#### `tabs_changed`

Отправляется при любом изменении состояния вкладок (создание, закрытие, навигация, перезагрузка, изменение заголовка или видимости). Это уведомление (без поля `id`), а не ответ.

```json
{
  "jsonrpc": "2.0",
  "method": "tabs_changed",
  "params": {}
}
```

Клиентам следует вызывать `list` для получения актуального состояния вкладок.

### Коды ошибок

- `-32600`: Invalid Request
- `-32601`: Method not found
- `-32602`: Invalid params
- `-32000`: Generic error (вкладка не найдена, не владелец, ошибка eval и т.д.)

## Тестирование через wkctl

CLI-утилита `wkctl` предоставляет удобный способ тестирования демона:

```bash
# Создать видимую вкладку
wkctl create --url https://example.com --visible

# Создать headless-вкладку
wkctl create --url https://example.com

# Список всех вкладок
wkctl list

# Показать вкладку в GTK-окне
wkctl show --id <tab-id>

# Скрыть вкладку (перевести в headless)
wkctl hide --id <tab-id>

# Навигация на новый URL
wkctl navigate --id <tab-id> --url https://example.org

# Перезагрузить вкладку
wkctl reload --id <tab-id>

# Выполнить JavaScript
wkctl eval --id <tab-id> --script 'document.title'

# Закрыть вкладку
wkctl close --id <tab-id>

# Завершить работу демона
wkctl shutdown
```

**Примечание:** `wkctl` использует фиксированный `client_id` (`00000000-0000-0000-0000-000000000000`), поэтому все вызовы `wkctl` разделяют владение вкладками, созданными через CLI.

## ewd-tui — Терминальный интерфейс

[`ewd-tui`](ewd-tui/README-ru.md) — полнофункциональный терминальный интерфейс для управления веб-проектами через `ewd-daemon`. Построен на `ratatui`, предоставляет:

- **Управление проектами** — создание, удаление и навигация по проектам через список
- **Авто-сборка** — отслеживание исходных файлов и пересборка через `esbuild` при изменениях
- **Интеграция с браузером** — автоматический запуск `ewd-daemon`, создание видимой вкладки и загрузка URL проекта
- **Инспектор сети** — просмотр сетевых запросов с таймингами, статус-кодами и фильтрацией
- **Консоль и логи** — вывод в реальном времени от сборки и логов демона
- **Несколько раскладок** — Emacs, Vim и VSCode (переключение через `Ctrl+K`)
- **Поиск** — фильтрация проектов и сетевых запросов

```bash
# Запуск терминального интерфейса
ewd-tui

# Или из рабочего пространства
cargo run --release -p ewd-tui
```

Полная документация: [ewd-tui/README-ru.md](ewd-tui/README-ru.md)

## Интеграция с Emacs

[`ewd.el`](emacs/README-ru.md) — Emacs Lisp клиент, подключающийся к демону через Unix сокет и предоставляющий:

- Управление вкладками внутри Emacs (буфер `*ewd-tabs*`)
- Выполнение JavaScript для веб-скрапинга и автоматизации
- Показ/скрытие вкладок (решение CAPTCHA)
- Push-обновления через уведомления `tabs_changed`
- Контроль доступа на основе владения

**Глобальные клавиши** (префикс `C-c w`):

| Клавиша    | Команда             | Описание                                   |
|------------|---------------------|--------------------------------------------|
| `C-c w c`  | `ewd-connect`       | Подключиться к демону                      |
| `C-c w t`  | `ewd-create-tab`    | Создать новую вкладку                      |
| `C-c w l`  | `ewd-list-tabs`     | Показать буфер `*ewd-tabs*`                |
| `C-c w k`  | `ewd-close-tab`     | Закрыть вкладку по ID                      |
| `C-c w e`  | `ewd-eval`          | Выполнить JavaScript во вкладке            |
| `C-c w n`  | `ewd-navigate`      | Перейти на новый URL во вкладке            |
| `C-c w r`  | `ewd-reload`        | Перезагрузить вкладку                      |
| `C-c w S`  | `ewd-show-tab`      | Показать вкладку в GTK-окне                |
| `C-c w H`  | `ewd-hide-tab`      | Скрыть вкладку (перевести в headless)      |

Дополнительные Emacs Lisp пакеты в [`elisp-packages/`](elisp-packages/):
- [`ewd-deepseek`](elisp-packages/ewd-deepseek/) — Интеграция с DeepSeek AI
- [`ewd-translate`](elisp-packages/ewd-translate/) — Плагин перевода

Полная документация: [emacs/README-ru.md](emacs/README-ru.md)

## Тестирование

### Unit-тесты

```bash
cargo test
```

### Интеграционные тесты

```bash
./bash/integration.sh
```

Подробнее: [bash/README.md](bash/README.md)

### Наборы тестов

| Набор        | Описание                                              |
|--------------|-------------------------------------------------------|
| `basic`      | Жизненный цикл вкладок (создание, список, закрытие)   |
| `eval`       | Выполнение JavaScript                                 |
| `errors`     | Обработка ошибок (несуществующая вкладка, UUID)       |
| `concurrent` | 10 параллельных вкладок (создание/закрытие)           |
| `orphan`     | Очистка осиротевших вкладок (~65с, интервал 60с)      |
| `shutdown`   | Корректное завершение демона                          |

## Документация

- [Спецификация протокола](docs/PROTOCOL.md) — детали протокола JSON-RPC
- [Справочник событий](docs/EVENTS.md) — документация системы событий

## Решение проблем

### Отказано в доступе к сокету

Убедитесь, что ваш пользователь имеет права на чтение/запись сокета:

```bash
ls -l $XDG_RUNTIME_DIR/webkit-bridge.sock
```

Демон создаёт сокет с правами вашего пользователя. При запуске от другого пользователя скорректируйте права или используйте того же пользователя.

### WebKitGTK падает на Wayland

Установите переменные окружения из раздела «Переменные окружения». Они отключают проблемные функции GPU-ускорения.

### Демон не запускается — сокет уже существует

Если демон ранее аварийно завершился, файл сокета может остаться. Удалите его вручную:

```bash
rm $XDG_RUNTIME_DIR/webkit-bridge.sock
```

Или демон автоматически очистит его при запуске.

### Вкладки не закрываются после отключения клиента

Вкладки принадлежат процессу клиента (идентифицируются по PID). Если процесс неожиданно завершился, фоновая задача очистки демона (запускается каждые 60 секунд) обнаружит мёртвый PID и автоматически закроет осиротевшие вкладки.

### ewd-tui: демон не найден

Убедитесь, что `ewd-daemon` собран и находится в PATH, или установите `EWD_DAEMON_PATH`:

```bash
export EWD_DAEMON_PATH=/path/to/ewd-daemon
ewd-tui
```

## Лицензия

Этот проект распространяется под лицензией MIT. Подробнее: [LICENSE](LICENSE).

## Участие в разработке

Вклад приветствуется! Пожалуйста, создавайте issue или отправляйте pull request.

1. Форкните репозиторий
2. Создайте ветку для новой функциональности
3. Протестируйте изменения: `cargo test` и `./bash/integration.sh`
4. Отправьте pull request
