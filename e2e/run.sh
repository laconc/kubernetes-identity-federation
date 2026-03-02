#!/usr/bin/env bash
# Run all tests in tests/
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

TESTS_DIR="$SCRIPT_DIR/tests"

echo "================================================"
echo "  kubernetes-identity-federation e2e test suite"
echo "================================================"
echo ""

OVERALL_EXIT=0

for test_file in "$TESTS_DIR"/[0-9][0-9]-*.sh; do
  test_name="$(basename "$test_file" .sh)"
  echo "── $test_name ──────────────────────────────────"
  # Run each test in a subshell so failures don't abort the runner.
  # Each test script sources lib.sh and increments PASS_COUNT/FAIL_COUNT,
  # but since they run in a subshell, we capture the exit code instead.
  if bash "$test_file"; then
    echo -e "${GREEN}[SUITE PASS]${NC} $test_name"
  else
    echo -e "${RED}[SUITE FAIL]${NC} $test_name"
    OVERALL_EXIT=1
  fi
  echo ""
done

echo "================================================"
if (( OVERALL_EXIT == 0 )); then
  echo -e "${GREEN}All tests passed.${NC}"
else
  echo -e "${RED}One or more tests failed.${NC}"
fi
echo "================================================"

exit "$OVERALL_EXIT"
