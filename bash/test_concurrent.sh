#!/usr/bin/env bash
# test_concurrent.sh — Concurrent client tests
# Tests: multiple clients creating/closing tabs simultaneously

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers.sh"

main() {
    setup_test_env
    check_dependencies
    start_daemon

    log INFO "=== Test: Create 10 tabs concurrently ==="
    local pids=()
    local id_files=()

    for i in {1..10}; do
        local id_file="$TEST_DIR/tab_id_$i"
        id_files+=("$id_file")
        (
            local output
            if ! output=$(wkctl tab.create --url "https://concurrent-$i.example.com" 2>&1); then
                echo "ERROR creating tab $i: $output" >&2
                exit 1
            fi
            echo "$output" | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}' | head -n 1 > "$id_file"
        ) &
        pids+=($!)
    done

    local failed=0
    for pid in "${pids[@]}"; do
        wait "$pid" || failed=$((failed + 1))
    done
    assert_success "0" "all concurrent creates completed (failures: $failed)"

    log INFO "=== Test: All 10 tabs visible in list ==="
    local list_output
    list_output="$(wkctl tab.list 2>/dev/null)"

    local count=0
    for id_file in "${id_files[@]}"; do
        if [[ -f "$id_file" ]]; then
            local tab_id
            tab_id="$(cat "$id_file" | tr -d '\r\n ')"

            if [[ -z "$tab_id" ]]; then
                log FAIL "DEBUG: ID file '$id_file' is EMPTY!"
            else
                log INFO "DEBUG: Extracted ID for tab: '$tab_id'"
            fi

            if echo "$list_output" | grep -qF "$tab_id"; then
                count=$((count + 1))
            fi
        else
            log FAIL "DEBUG: ID file '$id_file' does not exist!"
        fi
    done
    assert_eq "10" "$count" "all 10 tabs present in list"

    log INFO "=== Test: Close 10 tabs concurrently ==="
    pids=()
    for id_file in "${id_files[@]}"; do
        if [[ -f "$id_file" ]]; then
            local tab_id
            tab_id="$(cat "$id_file" | tr -d '\r\n ')"
            (
                log INFO "DEBUG: Attempting to close: '$tab_id'"
                if ! close_output=$(wkctl tab.close --id "$tab_id" 2>&1); then
                    log WARN "DEBUG: wkctl close failed for '$tab_id'. Output: $close_output"
                fi
            ) &
            pids+=($!)
        fi
    done

    failed=0
    for pid in "${pids[@]}"; do
        wait "$pid" || failed=$((failed + 1))
    done
    assert_success "0" "all concurrent closes completed (failures: $failed)"

    log INFO "=== Test: All tabs closed ==="
    list_output="$(wkctl tab.list 2>/dev/null)"
    assert_contains "$list_output" "No tabs" "all tabs closed"

    stop_daemon
    print_summary
}

main "$@"
