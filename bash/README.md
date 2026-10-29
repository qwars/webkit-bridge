# EWD Integration Tests

Bash-based integration tests for the EWD (Embedded WebKit Daemon) system.

## Overview

These tests verify the complete system behavior by:
- Starting a real `ewd-daemon` instance
- Communicating via Unix socket using `wkctl` client
- Testing tab lifecycle, JavaScript evaluation, error handling, concurrency, orphan cleanup, and graceful shutdown

## Prerequisites

- `bash` 4.0+
- Built binaries: `target/debug/wkctl` and `target/debug/ewd-daemon`

Build them with:
```bash
cargo build
```

## Usage

### Run all tests

```bash
./bash/integration.sh
```

### Run specific test suite

```bash
./bash/integration.sh basic
./bash/integration.sh eval
./bash/integration.sh errors
./bash/integration.sh concurrent
./bash/integration.sh orphan
./bash/integration.sh shutdown
```

## Test Suites

### `test_basic.sh` — Basic tab lifecycle
- Create a tab
- List tabs
- Close a tab
- Verify tab is removed
- Create multiple tabs
- Close one of multiple tabs

### `test_eval.sh` — JavaScript evaluation
- Eval simple expression (`1 + 1`)
- Eval string concatenation
- Eval `document.title`
- Eval DOM manipulation
- Eval with syntax error (should handle gracefully)

### `test_errors.sh` — Error handling
- Close non-existent tab
- Eval on non-existent tab
- Invalid UUID format
- Empty UUID
- List on empty daemon
- Double close

### `test_concurrent.sh` — Concurrent clients
- Create 10 tabs concurrently
- Verify all tabs visible
- Close 10 tabs concurrently
- Verify all tabs closed

### `test_orphan.sh` — Orphan cleanup (~65 seconds)
- Create a tab from a subshell client
- Kill the client process
- Wait for 60s cleanup interval
- Verify tab is automatically closed

### `test_shutdown.sh` — Graceful shutdown
- Verify daemon is alive
- Send `shutdown` via JSON-RPC
- Verify daemon stops within 5 seconds
- Verify socket file is removed

## Architecture

### Isolation Strategy

Each test suite runs in an isolated environment:
- Creates a temporary directory: `/tmp/ewd-test-$$/`
- Sets `EWD_SOCKET_PATH` to `<temp-dir>/webkit-bridge.sock`
- Preserves original `XDG_RUNTIME_DIR` for GTK initialization
- Both daemon and client automatically use the isolated socket path
- No interference with production daemon or other test runs

### Test Framework

`helpers.sh` provides:
- Assertion functions: `assert_eq`, `assert_contains`, `assert_not_empty`, `assert_success`, `assert_failure`
- Daemon management: `start_daemon`, `stop_daemon`, `daemon_is_alive`
- Environment setup: `setup_test_env`, `cleanup`
- Logging with colors and timestamps

### Cleanup

All tests register a `trap cleanup EXIT` to ensure:
- Daemon is stopped
- Temporary directory is removed
- No leftover socket files

## Debugging

### View logs

```bash
cat bash/test.log
```

### Run with verbose output

```bash
bash -x ./bash/integration.sh basic
```

### Manual testing

```bash
# Start daemon in one terminal
./target/debug/ewd-daemon

# In another terminal
./target/debug/wkctl create --url https://example.com
./target/debug/wkctl list
./target/debug/wkctl eval --id <tab-id> --script 'document.title'
./target/debug/wkctl close --id <tab-id>
```

## Known Limitations

- **Orphan test takes ~65 seconds** — This is expected due to the 60-second cleanup interval in the daemon
- **No GUI verification** — Tests verify behavior via IPC, not visual inspection
- **Single daemon instance** — Tests assume no other daemon is running on the system

## Adding New Tests

1. Create `bash/test_<name>.sh`
2. Source `helpers.sh`: `source "$SCRIPT_DIR/helpers.sh"`
3. Implement `main()` function
4. Add to `integration.sh` case statement
5. Run: `./bash/integration.sh <name>`

## CI Integration

These tests can be run in CI:

```yaml
- run: cargo build
- run: ./bash/integration.sh
```

Note: CI environments may require headless display setup (e.g., `xvfb-run` or Wayland-compatible environment).
