# webkit-bridge

Высокопроизводительный, легковесный демон WebKitGTK для автоматизации браузера в headless и видимом режимах, управляемый через JSON-RPC 2.0 over Unix sockets.

## Обзор

**webkit-bridge** — это проект на Rust, предоставляющий демон (`ewd-daemon`) и CLI-утилиту (`wkctl`) для управления экземплярами браузера WebKitGTK через чистый JSON-RPC API. Предназначен для интеграции с Emacs, скриптами и другими инструментами автоматизации.

### Ключевые особенности

- **Изолированное управление вкладками** с автоматической очисткой осиротевших вкладок
- **Контроль доступа на основе клиентов** — каждый клиент может управлять только созданными им вкладками
- **Поддержка прокси** (SOCKS5, HTTP) для анонимного просмотра
- **Headless и видимый режимы** для гибкой автоматизации
- **Уведомления о событиях в реальном времени** через JSON-RPC notifications
- **Zero-copy Unix socket IPC** для максимальной производительности

### Архитектура

```
webkit-bridge/
├── ewd-core/          # Библиотечный крейт (переиспользуемый API)
├── ewd-daemon/        # Бинарный крейт (автономный демон)
├── wkctl/             # CLI-утилита для тестирования
└── emacs/             # Emacs-клиент (отдельный проект)
```

## Системные требования

### Поддерживаемые платформы
- **ОС**: Linux (тестировалось на Debian 13, должно работать на любом современном дистрибутиве Linux)
- **Дисплей**: Wayland или X11
- **GPU**: Любой GPU с правильными драйверами (тестировалось на AMDGPU, Intel, NVIDIA)

### Необходимые зависимости

**Системные пакеты** (пример для Debian/Ubuntu):
```bash
sudo apt install \
  libwebkitgtk-6.0-dev \
  libgtk-4-dev \
  libsoup-3.0-dev \
  libjavascriptcoregtk-6.0-dev \
  pkg-config \
  build-essential
```

**Rust toolchain**:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Переменные окружения

**Рекомендуется для стабильности на Wayland/AMDGPU** (опционально, но рекомендуется):
```bash
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export GDK_BACKEND=wayland
```

Добавьте эти переменные в `~/.profile`, `~/.bashrc` или файл systemd-сервиса.

## Установка

### Сборка из исходников

```bash
git clone <repository-url>
cd webkit-bridge
cargo build --release
```

Бинарные файлы будут в `target/release/`:
- `ewd-daemon` — основной демон
- `wkctl` — CLI-утилита

### Установка в систему (опционально)

```bash
sudo cp target/release/ewd-daemon /usr/local/bin/
sudo cp target/release/wkctl /usr/local/bin/
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
2. Создание Unix-сокета в `$XDG_RUNTIME_DIR/webkit-bridge.sock` (или `/tmp/webkit-bridge.sock`)
3. Прослушивание JSON-RPC соединений
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

## Справочник JSON-RPC API

Демон взаимодействует через **JSON-RPC 2.0** over Unix domain sockets, используя формат **Newline-Delimited JSON (NDJSON)**.

**Путь к сокету**: `$XDG_RUNTIME_DIR/webkit-bridge.sock`

### Идентификация клиента

Каждое соединение получает уникальный `client_id` (UUID). Клиенты могут переопределить его, включив `"client_id"` в параметры запроса (используется `wkctl` с фиксированным ID `00000000-0000-0000-0000-000000000000`).

### Методы

#### `tab.create`

Создать новую вкладку браузера.

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.create",
  "params": {
    "url": "https://example.com",
    "visible": true,
    "profile": "default"
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

**Параметры:**
- `url` (string, опционально): URL для загрузки. По умолчанию: `"about:blank"`
- `visible` (boolean, опционально): Показать GTK-окно. По умолчанию: `false`
- `profile` (string, опционально): Имя профиля. По умолчанию: `"default"`

---

#### `tab.close`

Закрыть существующую вкладку. **Только клиент-владелец может закрывать свои вкладки.**

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.close",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9"
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

---

#### `tab.navigate`

Перейти по новому URL в вкладке. **Только клиент-владелец может навигировать свои вкладки.**

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.navigate",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "url": "https://example.org"
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

---

#### `tab.reload`

Перезагрузить вкладку. **Только клиент-владелец может перезагружать свои вкладки.**

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.reload",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9"
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

---

#### `tab.eval`

Выполнить JavaScript в контексте вкладки. **Только клиент-владелец может выполнять JS в своих вкладках.**

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.eval",
  "params": {
    "id": "ac93f48c-9b3b-4b64-8852-966d2af207b9",
    "script": "document.title"
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

---

#### `tab.list`

Список всех открытых вкладок. **Доступно всем клиентам (без проверки владельца).**

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "tab.list",
  "params": {},
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
      "owner": "00000000"
    },
    {
      "id": "bb91aed9-1d07-46ef-87ae-f4d75689881c",
      "url": "https://google.com",
      "title": "Google",
      "owner": "eff7e05a"
    }
  ],
  "id": 6
}
```

**Поля:**
- `id` (string): UUID вкладки
- `url` (string): Текущий URL
- `title` (string): Заголовок страницы
- `owner` (string): Короткий UUID (первые 8 символов) клиента-владельца

---

#### `daemon.shutdown`

Корректно завершить работу демона.

**Запрос:**
```json
{
  "jsonrpc": "2.0",
  "method": "daemon.shutdown",
  "params": {},
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

### Коды ошибок

- `-32600`: Неверный запрос
- `-32601`: Метод не найден
- `-32602`: Неверные параметры
- `-32000`: Общая ошибка (вкладка не найдена, не владелец, ошибка eval и т.д.)

## Тестирование через wkctl

CLI-утилита `wkctl` предоставляет удобный способ тестирования демона:

```bash
# Создать видимую вкладку
wkctl create --visible https://example.com

# Список всех вкладок
wkctl list

# Перейти по новому URL
wkctl navigate <tab-id> https://example.org

# Перезагрузить вкладку
wkctl reload <tab-id>

# Выполнить JavaScript
wkctl eval <tab-id> "document.title"

# Закрыть вкладку
wkctl close <tab-id>
```

**Примечание:** `wkctl` использует фиксированный `client_id` (`00000000-0000-0000-0000-000000000000`), поэтому все вызовы `wkctl` совместно владеют вкладками, созданными через CLI.

## Решение проблем

### Отказано в доступе к сокету

Убедитесь, что ваш пользователь имеет права на чтение/запись сокета:
```bash
ls -l $XDG_RUNTIME_DIR/webkit-bridge.sock
```

Демон создает сокет с правами вашего пользователя. Если запускаете от другого пользователя, измените права или используйте того же пользователя.

### WebKitGTK падает на Wayland

Установите переменные окружения, упомянутые в разделе "Переменные окружения". Они отключают проблемные функции GPU-ускорения.

### Демон не запускается — сокет уже существует

Если демон ранее аварийно завершился, файл сокета может всё ещё существовать. Удалите его вручную:
```bash
rm $XDG_RUNTIME_DIR/webkit-bridge.sock
```

Или демон автоматически очистит его при запуске.

### Вкладки не закрываются после отключения клиента

Вкладки принадлежат клиентскому процессу (идентифицируются по PID). Если процесс неожиданно умирает, фоновая задача очистки демона (запускается каждые 60 секунд) обнаружит мёртвый PID и автоматически закроет осиротевшие вкладки.

## Лицензия



## Участие в разработке
