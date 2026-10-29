#!/usr/bin/env bash
# test_errors.sh — Error handling tests
# Tests: invalid UUIDs, non-existent tabs, wrong client_id, malformed requests

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers.sh"

main() {
    setup_test_env
    check_dependencies
    start_daemon

    log INFO "=== Test: Close non-existent tab ==="
    local output
    local exit_code=0
    output="$(wkctl tab.close --id "00000000-0000-0000-0000-000000000000" 2>&1)" || exit_code=$?
    log INFO "Output: $output (exit: $exit_code)"
    assert_failure "$exit_code" "closing non-existent tab fails"

    log INFO "=== Test: Eval on non-existent tab ==="
    exit_code=0
    output="$(wkctl tab.eval --id "00000000-0000-0000-0000-000000000000" --script '1' 2>&1)" || exit_code=$?
    log INFO "Output: $output (exit: $exit_code)"
    assert_failure "$exit_code" "eval on non-existent tab fails"

    log INFO "=== Test: Invalid UUID format ==="
    exit_code=0
    output="$(wkctl tab.close --id "not-a-uuid" 2>&1)" || exit_code=$?
    log INFO "Output: $output (exit: $exit_code)"
    assert_failure "$exit_code" "invalid UUID format fails"

    log INFO "=== Test: Empty UUID ==="
    exit_code=0
    output="$(wkctl tab.close --id "" 2>&1)" || exit_code=$?
    log INFO "Output: $output (exit: $exit_code)"
    assert_failure "$exit_code" "empty UUID fails"

    log INFO "=== Test: List on empty daemon ==="
    output="$(wkctl tab.list)"
    log INFO "Output: $output"
    assert_contains "$output" "No tabs" "list on empty daemon shows no tabs"

    log INFO "=== Test: Create tab, close it, try to close again ==="
    local create_output
    create_output="$(wkctl tab.create --url https://example.com)"
    local tab_id
    tab_id="$(echo "$create_output" | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}')"
    log INFO "Created tab ID: $tab_id"

    wkctl tab.close --id "$tab_id" >/dev/null

    exit_code=0
    output="$(wkctl tab.close --id "$tab_id" 2>&1)" || exit_code=$?
    log INFO "Output: $output (exit: $exit_code)"
    assert_failure "$exit_code" "double close fails"

    stop_daemon
    print_summary
}

main "$@"
