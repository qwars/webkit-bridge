#!/usr/bin/env bash
# test_basic.sh — Basic tab lifecycle tests
# Tests: tab.create, tab.list, tab.close

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers.sh"

main() {
    setup_test_env
    check_dependencies
    start_daemon

    log INFO "=== Test: Create a tab ==="
    local create_output
    create_output="$(wkctl tab.create --url https://example.com)"
    log INFO "Create output: $create_output"
    assert_contains "$create_output" "Tab created:" "tab.create returns success message"

    # Извлекаем UUID с помощью строгого regex (без зависимости от кавычек)
    local tab_id
    tab_id="$(echo "$create_output" | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}')"
    assert_not_empty "$tab_id" "tab ID extracted"
    log INFO "Extracted Tab ID: $tab_id"

    log INFO "=== Test: List tabs ==="
    local list_output
    list_output="$(wkctl tab.list)"
    log INFO "List output: $list_output"
    assert_contains "$list_output" "$tab_id" "tab.list contains created tab ID"
    assert_contains "$list_output" "https://example.com" "tab.list contains URL"

    log INFO "=== Test: Close the tab ==="
    local close_output
    close_output="$(wkctl tab.close --id "$tab_id")"
    log INFO "Close output: $close_output"
    assert_contains "$close_output" "OK" "tab.close returns success"

    log INFO "=== Test: Verify tab is gone ==="
    list_output="$(wkctl tab.list)"
    log INFO "List output after close: $list_output"
    assert_contains "$list_output" "No tabs" "tab.list shows no tabs after close"

    log INFO "=== Test: Create multiple tabs ==="
    local id1 id2 id3
    id1="$(wkctl tab.create --url https://one.example.com | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}')"
    id2="$(wkctl tab.create --url https://two.example.com | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}')"
    id3="$(wkctl tab.create --url https://three.example.com | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}')"

    list_output="$(wkctl tab.list)"
    assert_contains "$list_output" "$id1" "first tab in list"
    assert_contains "$list_output" "$id2" "second tab in list"
    assert_contains "$list_output" "$id3" "third tab in list"

    log INFO "=== Test: Close one of multiple tabs ==="
    wkctl tab.close --id "$id2" >/dev/null
    list_output="$(wkctl tab.list)"
    assert_contains "$list_output" "$id1" "first tab still present"

    TESTS_RUN=$((TESTS_RUN + 1))
    if echo "$list_output" | grep -qF "$id2"; then
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log FAIL "✗ closed tab $id2 still in list"
    else
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log PASS "✓ closed tab $id2 removed from list"
    fi
    assert_contains "$list_output" "$id3" "third tab still present"

    log INFO "=== Cleanup remaining tabs ==="
    wkctl tab.close --id "$id1" >/dev/null || true
    wkctl tab.close --id "$id3" >/dev/null || true

    stop_daemon
    print_summary
}

main "$@"
