#!/usr/bin/env bash
# helpers.sh — Shared helper functions for integration tests
# Source this file from other test scripts: source "$(dirname "$0")/helpers.sh"

set -euo pipefail

# ─── Colors ──────────────────────────────────────────────────────────────────
readonly COLOR_GREEN='\033[0;32m'
readonly COLOR_RED='\033[0;31m'
readonly COLOR_YELLOW='\033[0;33m'
readonly COLOR_BLUE='\033[0;34m'
readonly COLOR_RESET='\033[0m'

# ─── Paths ───────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TEST_DIR="/tmp/ewd-test-$$"
SOCKET_PATH="$TEST_DIR/webkit-bridge.sock"
LOG_FILE="$SCRIPT_DIR/test.log"
DAEMON_PID_FILE="$TEST_DIR/daemon.pid"

# Timeouts
RPC_TIMEOUT=5
ORPHAN_TIMEOUT=65

# Counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# ─── Logging ─────────────────────────────────────────────────────────────────
log() {
    local level="$1"
    shift
    local msg="$*"
    local timestamp
    timestamp="$(date '+%Y-%m-%d %H:%M:%S')"
    echo "[$timestamp] [$level] $msg" >> "$LOG_FILE"
    case "$level" in
        INFO)  echo -e "${COLOR_BLUE}[INFO]${COLOR_RESET} $msg" ;;
        PASS)  echo -e "${COLOR_GREEN}[PASS]${COLOR_RESET} $msg" ;;
        FAIL)  echo -e "${COLOR_RED}[FAIL]${COLOR_RESET} $msg" ;;
        WARN)  echo -e "${COLOR_YELLOW}[WARN]${COLOR_RESET} $msg" ;;
    esac
}

# ─── Test framework ──────────────────────────────────────────────────────────
assert_eq() {
    local expected="$1"
    local actual="$2"
    local msg="${3:-}"
    TESTS_RUN=$((TESTS_RUN + 1))
    if [[ "$expected" == "$actual" ]]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log PASS "✓ ${msg:-assertion passed} (expected: $expected)"
        return 0
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log FAIL "✗ ${msg:-assertion failed} (expected: '$expected', got: '$actual')"
        return 1
    fi
}

assert_contains() {
    local haystack="$1"
    local needle="$2"
    local msg="${3:-}"
    TESTS_RUN=$((TESTS_RUN + 1))
    if echo "$haystack" | grep -qF "$needle"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log PASS "✓ ${msg:-assertion passed} (contains: $needle)"
        return 0
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log FAIL "✗ ${msg:-assertion failed} (expected to contain: '$needle', got: '$haystack')"
        return 1
    fi
}

assert_not_empty() {
    local value="$1"
    local msg="${2:-}"
    TESTS_RUN=$((TESTS_RUN + 1))
    if [[ -n "$value" ]]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log PASS "✓ ${msg:-assertion passed} (non-empty)"
        return 0
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log FAIL "✗ ${msg:-assertion failed} (expected non-empty)"
        return 1
    fi
}

assert_success() {
    local exit_code="$1"
    local msg="${2:-}"
    TESTS_RUN=$((TESTS_RUN + 1))
    if [[ "$exit_code" -eq 0 ]]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log PASS "✓ ${msg:-command succeeded}"
        return 0
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log FAIL "✗ ${msg:-command failed} (exit code: $exit_code)"
        return 1
    fi
}

assert_failure() {
    local exit_code="$1"
    local msg="${2:-}"
    TESTS_RUN=$((TESTS_RUN + 1))
    if [[ "$exit_code" -ne 0 ]]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log PASS "✓ ${msg:-command failed as expected} (exit code: $exit_code)"
        return 0
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log FAIL "✗ ${msg:-command should have failed but succeeded}"
        return 1
    fi
}

# ─── wkctl wrapper ───────────────────────────────────────────────────────────
# Runs wkctl with the test socket path via XDG_RUNTIME_DIR
wkctl() {
    local cmd="$*"
    log INFO "wkctl $cmd"
    # XDG_RUNTIME_DIR is already set in the environment by setup_test_env
    timeout "$RPC_TIMEOUT" "$PROJECT_ROOT/target/debug/wkctl" "$@" 2>&1
}

# ─── Daemon management ───────────────────────────────────────────────────────
start_daemon() {
    log INFO "Starting daemon with socket: $SOCKET_PATH"
    # Ensure env vars for headless stability on AMDGPU/Wayland
    export WEBKIT_DISABLE_COMPOSITING_MODE=1
    export WEBKIT_DISABLE_DMABUF_RENDERER=1
    export GDK_BACKEND=wayland

    "$PROJECT_ROOT/target/debug/ewd-daemon" &
    local pid=$!
    echo "$pid" > "$DAEMON_PID_FILE"
    log INFO "Daemon started with PID: $pid"

    # Wait for socket to appear
    local waited=0
    while [[ ! -S "$SOCKET_PATH" && $waited -lt 10 ]]; do
        sleep 0.5
        waited=$((waited + 1))
    done

    if [[ ! -S "$SOCKET_PATH" ]]; then
        log FAIL "Daemon failed to create socket at $SOCKET_PATH"
        return 1
    fi
    log INFO "Daemon socket ready"
}

stop_daemon() {
    if [[ -f "$DAEMON_PID_FILE" ]]; then
        local pid
        pid="$(cat "$DAEMON_PID_FILE")"
        if kill -0 "$pid" 2>/dev/null; then
            log INFO "Stopping daemon (PID: $pid)"
            kill "$pid" 2>/dev/null || true
            # Wait for graceful shutdown
            local waited=0
            while kill -0 "$pid" 2>/dev/null && [[ $waited -lt 10 ]]; do
                sleep 0.5
                waited=$((waited + 1))
            done
            if kill -0 "$pid" 2>/dev/null; then
                log WARN "Daemon didn't stop gracefully, sending SIGKILL"
                kill -9 "$pid" 2>/dev/null || true
            fi
        fi
        rm -f "$DAEMON_PID_FILE"
    fi
}

daemon_is_alive() {
    if [[ -f "$DAEMON_PID_FILE" ]]; then
        local pid
        pid="$(cat "$DAEMON_PID_FILE")"
        kill -0 "$pid" 2>/dev/null
        return $?
    fi
    return 1
}

# ─── Environment setup ───────────────────────────────────────────────────────
setup_test_env() {
    # Создаем изолированную тестовую директорию
    mkdir -p "$TEST_DIR"

    # НЕ перезаписываем XDG_RUNTIME_DIR, чтобы GTK мог найти Wayland/X11 сокет!
    # Вместо этого используем специальную переменную для нашего приложения
    export EWD_SOCKET_PATH="$TEST_DIR/webkit-bridge.sock"

    # Очищаем лог-файл
    > "$LOG_FILE"

    log INFO "Test environment: EWD_SOCKET_PATH=$EWD_SOCKET_PATH"
    log INFO "Original XDG_RUNTIME_DIR preserved for GTK: ${XDG_RUNTIME_DIR:-/tmp}"
    log INFO "Log file: $LOG_FILE"
}

cleanup() {
    log INFO "Cleaning up test environment"
    stop_daemon
    rm -rf "$TEST_DIR"
    log INFO "Cleanup complete"
}

# Register cleanup trap
trap cleanup EXIT

# ─── Dependency check ────────────────────────────────────────────────────────
check_dependencies() {
    local missing=0

    if ! command -v jq &>/dev/null; then
        log FAIL "jq is not installed"
        missing=1
    fi

    if [[ ! -x "$PROJECT_ROOT/target/debug/wkctl" ]]; then
        log FAIL "wkctl binary not found at $PROJECT_ROOT/target/debug/wkctl"
        log INFO "Run 'cargo build' first"
        missing=1
    fi

    if [[ ! -x "$PROJECT_ROOT/target/debug/ewd-daemon" ]]; then
        log FAIL "ewd-daemon binary not found at $PROJECT_ROOT/target/debug/ewd-daemon"
        log INFO "Run 'cargo build' first"
        missing=1
    fi

    if [[ $missing -ne 0 ]]; then
        exit 1
    fi

    log INFO "All dependencies satisfied"
}

# ─── Test summary ────────────────────────────────────────────────────────────
print_summary() {
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "${COLOR_BLUE}Test Summary${COLOR_RESET}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "Total:  $TESTS_RUN"
    echo -e "${COLOR_GREEN}Passed: $TESTS_PASSED${COLOR_RESET}"
    echo -e "${COLOR_RED}Failed: $TESTS_FAILED${COLOR_RESET}"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if [[ $TESTS_FAILED -eq 0 ]]; then
        echo -e "${COLOR_GREEN}✓ All tests passed${COLOR_RESET}"
        return 0
    else
        echo -e "${COLOR_RED}✗ Some tests failed${COLOR_RESET}"
        echo "See $LOG_FILE for details"
        return 1
    fi
}
