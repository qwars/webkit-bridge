#!/usr/bin/env bash
# test_orphan.sh — Orphan cleanup tests
# Tests: tabs are closed when owner process dies
# NOTE: This test takes ~65 seconds (60s cleanup interval + buffer)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers.sh"

main() {
    setup_test_env
    check_dependencies
    start_daemon

    log INFO "=== Test: Orphan cleanup (this takes ~65 seconds) ==="

    log INFO "1. Creating a tab from a background client process"
    (
        local output
        if ! output=$(wkctl create --url https://orphan-test.example.com 2>&1); then
            echo "ERROR creating orphan tab: $output" >&2
            exit 1
        fi
        local tab_id
        tab_id="$(echo "$output" | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}' | head -n 1)"
        echo "$tab_id" > "$TEST_DIR/orphan_tab_id"
        sleep 300
    ) &
    local client_pid=$!
    log INFO "   Background client started with PID: $client_pid"

    sleep 2

    local tab_id
    if [[ ! -f "$TEST_DIR/orphan_tab_id" ]]; then
        log FAIL "Failed to get orphan tab ID. Client output was not saved."
        kill "$client_pid" 2>/dev/null || true
        stop_daemon
        exit 1
    fi
    tab_id="$(cat "$TEST_DIR/orphan_tab_id" | tr -d '\r\n ')"
    log INFO "   Created orphan tab with ID: $tab_id"

    log INFO "2. Verifying tab exists before client death"
    local list_output
    list_output="$(wkctl list 2>/dev/null)"
    assert_contains "$list_output" "$tab_id" "orphan tab exists before client death"

    log INFO "3. Killing the client process (PID: $client_pid)"
    kill -9 "$client_pid" 2>/dev/null || true
    wait "$client_pid" 2>/dev/null || true
    log INFO "   Client process killed."

    log INFO "4. Waiting for orphan cleanup (60s interval + 5s buffer = 65s)"
    for i in {1..13}; do
        sleep 5
        log INFO "   ... waited $((i * 5))s / 65s"
    done

    log INFO "5. Verifying tab is gone after cleanup"
    list_output="$(wkctl list 2>/dev/null)"
    log INFO "   List output after cleanup: $list_output"
    assert_contains "$list_output" "No tabs" "orphan tab cleaned up after client death"

    stop_daemon
    print_summary
}

main "$@"
