# ewd-translate — Translation via EWD and translate.ru

An Emacs Lisp package that provides **translation of words, regions, and buffers** using [translate.ru](https://www.translate.ru) via the EWD (Emacs WebKit Daemon) browser automation system.

**Package version:** 0.1.0  
**Requires:** Emacs 28.1+, [ewd](../ewd/) 0.8.0+

## Overview

`ewd-translate` leverages the EWD browser automation framework to open a hidden WebKitGTK tab with translate.ru, inject JavaScript that hooks into the translation engine, and receive richly formatted results — including dictionary entries with transcriptions, part-of-speech tags, usage examples, and multiple translation variants — directly into an Emacs buffer.

Unlike traditional Emacs translation packages that rely on CLI tools or simple HTTP APIs, `ewd-translate` uses the full web interface of translate.ru, giving you access to its complete dictionary data and example sentences.

## How It Works

1. **Tab management** — On first use, `ewd-translate` searches for an existing translate.ru tab or creates a new one via EWD
2. **JS injection** — Once the page loads, custom JavaScript is injected that:
   - Hooks into `window.trru.translation.getTranslationDone` to intercept results
   - Provides `window.ewdTranslate(text)` to trigger translations programmatically
   - Parses dictionary HTML (`parseDictView`) into structured text
   - Sends results back to Emacs via `window.ewd.send('translate_result', data)`
3. **Chunked processing** — Large texts are split into chunks (default max 4000 chars) and translated sequentially
4. **Result rendering** — Results are displayed in the `*EWD Translate*` buffer with semantic fontification

## Features

### Translation Commands

| Command                | Keybinding | Description               |
|------------------------|------------|---------------------------|
| `ewd-translate-word`   | `C-c t w`  | Translate word at point   |
| `ewd-translate-region` | `C-c t r`  | Translate selected region |
| `ewd-translate-buffer` | `C-c t b`  | Translate entire buffer   |

### Dictionary View

When translating a word or short phrase, translate.ru returns a full dictionary entry including:

- **Word form** with transcription (e.g., `evolution [ˌevəˈluːʃn]`)
- **Part of speech and grammatical forms** (e.g., noun, verb conjugations)
- **Translation variants** with reference information
- **Usage examples** with source and translation side by side

### Text Handling

- **Chunked translation** — large texts are split into manageable chunks (configurable, default 4000 chars)
- **Hyphenation normalization** — words broken across lines by soft hyphens are rejoined
- **Whitespace normalization** — multiple spaces are collapsed; newlines are preserved
- **Progress indication** — pending chunks are shown as `[ожидает...]` until translated

### Buffer Display

The `*EWD Translate*` buffer uses `special-mode` with custom faces:

| Face                             | Applied to                                   |
|----------------------------------|----------------------------------------------|
| `ewd-translate-header-face`      | Session headers (`=== Label ===`)            |
| `ewd-translate-form-face`        | Dictionary word forms + transcription        |
| `ewd-translate-translation-face` | Translation lines                            |
| `ewd-translate-ref-face`         | Reference info (part of speech, usage notes) |
| `ewd-translate-example-face`     | Usage example lines                          |
| `ewd-translate-separator-face`   | Separator lines (`────`)                     |
| `ewd-translate-error-face`       | Error messages                               |
| `ewd-translate-pending-face`     | Pending chunk placeholders                   |

### Automation

- **Auto-start** — on Emacs startup, the package automatically connects to EWD and prepares the translation tab
- **Tab reuse** — if a translate.ru tab already exists, it is reused instead of creating a new one
- **Queued operations** — if the translation tab is not yet ready, translation requests are queued and executed when ready

## Requirements

- Emacs 28.1+ (with `json-parse-string` support)
- [ewd](../ewd/) 0.8.0+ — the EWD client package (loaded automatically)
- `ewd-daemon` — the WebKitGTK daemon (must be built and available in `PATH`)
- Internet connection (to access translate.ru)

See the [webkit-bridge README](../../README.md) for daemon installation instructions.

## Installation

### Manual Installation

Copy the `ewd-translate/` directory to your Emacs load path:

```bash
mkdir -p ~/.emacs.d/lisp
cp -r ewd-translate/ ~/.emacs.d/lisp/
```

Add to your `init.el`:

```elisp
(add-to-list 'load-path "~/.emacs.d/lisp/ewd-translate")
(require 'ewd-translate)
```

### Using use-package

```elisp
(use-package ewd-translate
  :load-path "~/.emacs.d/lisp/ewd-translate"
  :after ewd)
```

## Configuration

All customization options are available via `M-x customize-group RET ewd-translate RET`:

| Variable                          | Default                              | Description                                                                  |
|-----------------------------------|--------------------------------------|------------------------------------------------------------------------------|
| `ewd-translate-max-chunk-length`  | `4000`                               | Maximum length of a text chunk for translation                               |
| `ewd-translate-url`               | `"https://www.translate.ru/перевод"` | URL of the translation service                                               |
| `ewd-translate-js`                | `""` (uses built-in)                 | Custom JavaScript code or path to a JS file to inject instead of the default |
| `ewd-translate-page-load-timeout` | `30`                                 | Seconds to wait for the translation page to finish loading                   |

### Custom JavaScript

If you want to use a different translation service or customize the injection logic, set `ewd-translate-js` to:

- A string containing raw JavaScript code, or
- A file path pointing to a `.js` file

When empty (default), the built-in injection script is used, which is specifically designed for translate.ru.

### Example Configuration

```elisp
(use-package ewd-translate
  :load-path "~/.emacs.d/lisp/ewd-translate"
  :after ewd
  :custom
  (ewd-translate-max-chunk-length 2000)
  (ewd-translate-page-load-timeout 45))
```

## Usage

### Basic Usage

1. **Ensure the daemon is running** (EWD will auto-start it if in PATH):
   ```bash
   ewd-daemon
   ```

2. **Translate a word** — place cursor on a word and press `C-c t w`:
   ```
   Results in *EWD Translate*:
   === Word: evolution ===
   evolution [ˌevəˈluːʃn]
       эволюция, развитие, evolvent, involute
       › The theory of evolution is well established.
       › Теория эволюции хорошо обоснована.
   ```

3. **Translate a region** — select text and press `C-c t r`:
   ```
   === Region (127 chars) ===
   [ожидает...]
   [ожидает...]
   ```

   Results appear incrementally as chunks are processed:
   ```
   === Region (127 chars) / 2 chunks ===
   This is the first chunk...
   Это первый фрагмент...

   This is the second chunk...
   Это второй фрагмент...
   ```

4. **Translate a buffer** — press `C-c t b` to translate the entire current buffer.

### Keybindings

| Keybinding | Command                | Description               |
|------------|------------------------|---------------------------|
| `C-c t w`  | `ewd-translate-word`   | Translate word at point   |
| `C-c t r`  | `ewd-translate-region` | Translate selected region |
| `C-c t b`  | `ewd-translate-buffer` | Translate entire buffer   |

### Viewing Results

The `*EWD Translate*` buffer opens automatically when a translation completes. It is read-only (`special-mode`) with fontified content. Use standard Emacs navigation to browse the results.

## Examples

### Translate a Word

```elisp
;; Place cursor on the word "evolution" and:
M-x ewd-translate-word
;; or press: C-c t w

;; Output in *EWD Translate*:
;; === Word: evolution ===
;; evolution [ˌevəˈluːʃn]
;;     эволюция, развитие
;;     › The theory of evolution
;;     › Теория эволюции
```

### Translate a Region Programmatically

```elisp
;; Select text in the current buffer, then:
(ewd-translate-region (region-beginning) (region-end))

;; Or programmatically with a string argument:
(let ((text "The quick brown fox jumps over the lazy dog."))
  (with-temp-buffer
    (insert text)
    (ewd-translate-region (point-min) (point-max))))
```

### Customize Chunk Size

```elisp
;; For shorter, more responsive translations:
(setq ewd-translate-max-chunk-length 1000)

;; For fewer API calls (larger chunks):
(setq ewd-translate-max-chunk-length 8000)
```

## Troubleshooting

### "EWD daemon not connected"

Ensure the `ewd-daemon` is running and EWD is connected:

```bash
ewd-daemon
```

Then run `M-x ewd-connect` in Emacs before attempting translation.

### Translation Tab Not Loading

- Check your internet connection
- Verify that translate.ru is accessible
- Increase `ewd-translate-page-load-timeout` if the page is slow to load
- Check the `*EWD Debug*` buffer (from `ewd-debug.el`) for IPC logs

### "Empty text" Error

The region or buffer you're trying to translate contains no translatable content. Select a non-empty region or move point to a word.

### Translation Results Are Empty

- Ensure JavaScript is enabled in the WebKitGTK tab (it is by default)
- Check if translate.ru has changed its page structure (the JS injection may need updating)
- Try setting a custom `ewd-translate-js` to adapt to page changes

### Timeout Error

If the translation page takes too long to load:

```elisp
(setq ewd-translate-page-load-timeout 60)
```

## Architecture

```
┌───────────────────────────────────────────────────────┐
│                     Emacs                             │
│  ┌────────────────────────────────────────────────┐   │
│  │           ewd-translate.el                     │   │
│  │                                                │   │
│  │  Commands:    Processing:     Display:         │   │
│  │  - word       - chunk split   *EWD Translate*  │   │
│  │  - region     - hyphen fix    buffer with      │   │
│  │  - buffer     - queue mgmt    fontification    │   │
│  └──────────┬─────────────────────────────────────┘   │
│             │ JS bridge (ewd-register-js-handler)     │
│  ┌──────────┴─────────────────────────────────────┐   │
│  │                ewd.el (core)                   │   │
│  │    JSON-RPC 2.0 over Unix socket               │   │
│  └──────────┬─────────────────────────────────────┘   │
└─────────────┼─────────────────────────────────────────┘
              │
    Unix Socket (NDJSON)
              │
┌─────────────┼────────────────────────────────────────┐
│  ┌──────────┴────────────────────────────────────┐   │
│  │             ewd-daemon                        │   │
│  │         (WebKitGTK instance)                  │   │
│  │                                               │   │
│  │  ┌─────────────────────────────────────────┐  │   │
│  │  │  translate.ru (hidden tab)              │  │   │
│  │  │                                         │  │   │
│  │  │  Injected JS:                           │  │   │
│  │  │  - getTranslationDone hook              │  │   │
│  │  │  - parseDictView parser                 │  │   │
│  │  │  - ewdTranslate() function              │  │   │
│  │  └─────────────────────────────────────────┘  │   │
│  └───────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────┘
```

### Component Flow

1. User invokes a translation command (`ewd-translate-word`, `ewd-translate-region`, or `ewd-translate-buffer`)
2. Text is split into chunks (max `ewd-translate-max-chunk-length` characters each)
3. Chunks are sent sequentially via `ewd-eval-async` to the translate.ru tab, calling `window.ewdTranslate(text)`
4. The injected JS fills the translation form, clicks the translate button, and intercepts the result
5. Results (dictionary HTML or plain text) are sent back to Emacs via the JS bridge (`window.ewd.send`)
6. The `translate_result` handler updates the `*EWD Translate*` buffer with formatted, fontified output
7. The next chunk is sent automatically until all chunks are processed

## Related Projects

- [ewd](../ewd/) — Emacs WebKit Daemon client (required dependency)
- [ewd-deepseek](../ewd-deepseek/) — DeepSeek AI integration for EWD
- [webkit-bridge](../../README.md) — The daemon that powers EWD

## License

This project is licensed under the MIT License. See [LICENSE](../../LICENSE) for details.

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Test with Emacs 28.1+
4. Submit a pull request
