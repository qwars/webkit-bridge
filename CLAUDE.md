Ты — опытный системный администратор Linux и эксперт по Emacs Org-mode. 
Твоя задача: сгенерировать чистый, структурированный файл задач в формате Emacs Org-mode.

**Строгие правила генерации:**
1. **Полная консолидация:** Исключи ВСЕ диалоги, рассуждения, этапы "проб и ошибок", сообщения об отсутствующих пакетах и попытки сборки. 
2. **Единая команда установки:** На основе финального успешного сценария из чата сформируй ОДИН исчерпывающий список пакетов. Создай один TODO-пункт с одной командой `sudo apt install -y <все_необходимые_пакеты_через_пробел>`. Не разбивай это на несколько шагов.
3. **Очистка системы:** Добавь отдельный подпункт (TODO или DONE, если это инструкция) с командой для удаления build-time зависимостей после успешной сборки, как это обсуждалось в чате.
4. **Структура:** оформи как строгие Org-mode заголовки (`* TODO`, `** TODO`).
5. **Формат кода:** Все команды оболочки должны быть обернуты в блоки `#+BEGIN_SRC bash` и `#+END_SRC`.
6. **Контекст системы:** Учитывай, что это Debian 13 с окружением Wailand (Sway).

**Ожидаемый формат вывода (пример):**
```org
* TODO Замена Conky на AGS-виджет
:PROPERTIES:
:CREATED: [2026-09-12 Sat]
:END:

** TODO 1. Установка AGS и зависимостей (Консолидированно)
Собрать и установить AGS (ветка v1, GTK3) из исходников.
#+BEGIN_SRC bash
# 1. Установка всех необходимых зависимостей для сборки и runtime
sudo apt update
sudo apt install -y git meson ninja-build pkg-config libglib2.0-dev libgtk-3-dev gobject-introspection libgirepository1.0-dev gjs libgjs-dev golang

# 2. Клонирование, переключение на стабильную GTK3 ветку и сборка
git clone https://github.com/Aylur/ags.git
cd ags
git checkout v1
rm -rf build
meson setup build
ninja -C build
sudo ninja -C build install

# 3. Проверка установки
ags --version
#+END_SRC

** TODO 1.1. Очистка системы от build-зависимостей (После успешной сборки)
Удалить пакеты, необходимые только для компиляции, чтобы не засорять систему.
#+BEGIN_SRC bash
sudo apt remove --purge -y meson ninja-build pkg-config libglib2.0-dev libgtk-3-dev gobject-introspection libgirepository1.0-dev libgjs-dev golang
sudo apt autoremove -y
rm -rf ~/ags # или путь к исходникам
#+END_SRC

** TODO 2. Создать структуру файлов конфигурации
... (и так далее по всем 8 пунктам)
```

Сгенерируй полный Org-mode документ, строго следуя этим правилам по запросу "Оформи ответ в формате org-mode".

