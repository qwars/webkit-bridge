#!/usr/bin/env bash
# test_shutdown.sh — Graceful shutdown tests
# Tests: daemon.shutdown via IPC stops the daemon

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers.sh"

main() {
    setup_test_env
    check_dependencies
    start_daemon

    log INFO "=== Test: Daemon is alive before shutdown ==="
    if daemon_is_alive; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        TESTS_RUN=$((TESTS_RUN + 1))
        log PASS "✓ daemon is alive before shutdown"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        TESTS_RUN=$((TESTS_RUN + 1))
        log FAIL "✗ daemon should be alive before shutdown"
    fi

    log INFO "=== Test: Send daemon.shutdown command ==="
    # Use raw JSON-RPC via socat or netcat to send shutdown
    # wkctl doesn't have a shutdown command, so we send raw JSON
    local shutdown_request='{"jsonrpc":"2.0","method":"daemon.shutdown","params":{},"id":1}'

    # Use socat if available, otherwise use bash /dev/tcp
    if command -v socat &>/dev/null; then
        echo "$shutdown_request" | socat - UNIX-CONNECT:"$SOCKET_PATH" >/dev/null 2>&1 || true
    else
        # Fallback: use Python for socket communication
        python3 -c "
import socket, sys
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect('$SOCKET_PATH')
s.sendall(b'$shutdown_request')
s.close()
" 2>/dev/null || true
    fi

    log INFO "=== Test: Daemon stops within 5 seconds ==="
    local waited=0
    while daemon_is_alive && [[ $waited -lt 10 ]]; do
        sleep 0.5
        waited=$((waited + 1))
    done

    if daemon_is_alive; then
        TESTS_FAILED=$((TESTS_FAILED + 1))
        TESTS_RUN=$((TESTS_RUN + 1))
        log FAIL "✗ daemon should have stopped after shutdown command"
        # Force kill for cleanup
        stop_daemon
    else
        TESTS_PASSED=$((TESTS_PASSED + 1))
        TESTS_RUN=$((TESTS_RUN + 1))
        log PASS "✓ daemon stopped after shutdown command"
    fi

    log INFO "=== Test: Socket file removed ==="
    if [[ ! -S "$SOCKET_PATH" ]]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        TESTS_RUN=$((TESTS_RUN + 1))
        log PASS "✓ socket file removed after shutdown"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        TESTS_RUN=$((TESTS_RUN + 1))
        log FAIL "✗ socket file should be removed after shutdown"
    fi

    # Don't call stop_daemon again — daemon is already dead
    # Just remove PID file to prevent cleanup from trying to kill it
    rm -f "$DAEMON_PID_FILE"

    print_summary
}

main "$@"
