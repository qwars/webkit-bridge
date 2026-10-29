#!/usr/bin/env bash
# test_eval.sh — JavaScript evaluation tests
# Tests: tab.eval with various expressions

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers.sh"

main() {
    setup_test_env
    check_dependencies
    start_daemon

    log INFO "=== Setup: Create a tab ==="
    local create_output
    create_output="$(wkctl tab.create --url https://example.com)"
    local tab_id
    tab_id="$(echo "$create_output" | grep -oE '[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}')"
    assert_not_empty "$tab_id" "tab created for eval tests"
    log INFO "Extracted Tab ID: $tab_id"

    sleep 2

    log INFO "=== Test: Eval simple expression ==="
    local eval_output
    eval_output="$(wkctl tab.eval --id "$tab_id" --script '1 + 1')"
    log INFO "Eval output: $eval_output"
    assert_contains "$eval_output" "2" "eval returns correct result for 1+1"

    log INFO "=== Test: Eval string expression ==="
    eval_output="$(wkctl tab.eval --id "$tab_id" --script '"hello" + " " + "world"')"
    log INFO "Eval output: $eval_output"
    assert_contains "$eval_output" "hello world" "eval returns concatenated string"

    log INFO "=== Test: Eval document.title ==="
    eval_output="$(wkctl tab.eval --id "$tab_id" --script 'document.title')"
    log INFO "Eval output: $eval_output"
    assert_not_empty "$eval_output" "eval returns document title"

    log INFO "=== Test: Eval DOM manipulation ==="
    eval_output="$(wkctl tab.eval --id "$tab_id" --script 'document.body.innerHTML = "<h1>Test</h1>"; document.body.innerHTML')"
    log INFO "Eval output: $eval_output"
    assert_contains "$eval_output" "Test" "eval can manipulate DOM"

    log INFO "=== Test: Eval with syntax error ==="
    local eval_exit=0
    eval_output="$(wkctl tab.eval --id "$tab_id" --script 'this is not valid javascript {{{')" || eval_exit=$?
    log INFO "Eval error output: $eval_output (exit: $eval_exit)"

    TESTS_RUN=$((TESTS_RUN + 1))
    if [[ $eval_exit -ne 0 ]] || echo "$eval_output" | grep -qiE "(error|failed|invalid)"; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log PASS "✓ eval handles syntax error gracefully"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log FAIL "✗ eval should report syntax error"
    fi

    log INFO "=== Cleanup ==="
    wkctl tab.close --id "$tab_id" >/dev/null || true

    stop_daemon
    print_summary
}

main "$@"
