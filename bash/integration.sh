#!/usr/bin/env bash
# integration.sh — Main runner for all integration tests
# Usage: ./bash/integration.sh [test_name]
#   test_name: basic | eval | errors | concurrent | orphan | shutdown | all (default)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers.sh"

readonly COLOR_CYAN='\033[0;36m'

run_test_suite() {
    local suite_name="$1"
    local script_path="$SCRIPT_DIR/test_${suite_name}.sh"

    echo ""
    echo -e "${COLOR_CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${COLOR_RESET}"
    echo -e "${COLOR_CYAN}Running test suite: $suite_name${COLOR_RESET}"
    echo -e "${COLOR_CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${COLOR_RESET}"

    if [[ ! -f "$script_path" ]]; then
        log FAIL "Test suite not found: $script_path"
        return 1
    fi

    # Run in subshell to isolate environment (each suite gets its own XDG_RUNTIME_DIR)
    (
        bash "$script_path"
    )
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        log FAIL "Test suite '$suite_name' failed with exit code $exit_code"
        return $exit_code
    fi

    log PASS "Test suite '$suite_name' completed successfully"
    return 0
}

main() {
    local target="${1:-all}"

    echo -e "${COLOR_CYAN}╔═══════════════════════════════════════════════════╗${COLOR_RESET}"
    echo -e "${COLOR_CYAN}║     EWD Integration Test Runner                   ║${COLOR_RESET}"
    echo -e "${COLOR_CYAN}╚═══════════════════════════════════════════════════╝${COLOR_RESET}"
    echo ""

    check_dependencies

    # Ensure binaries are built
    if [[ ! -x "$PROJECT_ROOT/target/debug/wkctl" ]] || [[ ! -x "$PROJECT_ROOT/target/debug/ewd-daemon" ]]; then
        log INFO "Binaries not found, running 'cargo build'..."
        (cd "$PROJECT_ROOT" && cargo build)
    fi

    local overall_exit=0

    case "$target" in
        basic)
            run_test_suite "basic" || overall_exit=1
            ;;
        eval)
            run_test_suite "eval" || overall_exit=1
            ;;
        errors)
            run_test_suite "errors" || overall_exit=1
            ;;
        concurrent)
            run_test_suite "concurrent" || overall_exit=1
            ;;
        orphan)
            run_test_suite "orphan" || overall_exit=1
            ;;
        shutdown)
            run_test_suite "shutdown" || overall_exit=1
            ;;
        all)
            # Run in order of speed (fastest first, orphan last because it takes ~65s)
            for suite in basic eval errors concurrent shutdown orphan; do
                run_test_suite "$suite" || overall_exit=1
            done
            ;;
        *)
            echo "Usage: $0 [basic|eval|errors|concurrent|orphan|shutdown|all]"
            exit 1
            ;;
    esac

    echo ""
    if [[ $overall_exit -eq 0 ]]; then
        echo -e "${COLOR_GREEN}✓ All test suites passed${COLOR_RESET}"
    else
        echo -e "${COLOR_RED}✗ Some test suites failed${COLOR_RESET}"
    fi

    exit $overall_exit
}

main "$@"
