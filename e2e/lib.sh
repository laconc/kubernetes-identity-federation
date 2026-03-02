#!/usr/bin/env bash
# Shared helpers for e2e tests.
set -euo pipefail

# Colours
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

PASS_COUNT=0
FAIL_COUNT=0

log_info() { echo -e "${YELLOW}[INFO]${NC} $*"; }
log_pass() { echo -e "${GREEN}[PASS]${NC} $*"; PASS_COUNT=$((PASS_COUNT + 1)); }
log_fail() { echo -e "${RED}[FAIL]${NC} $*"; FAIL_COUNT=$((FAIL_COUNT + 1)); }

# assert_eq <expected> <actual> <message>
assert_eq() {
  local expected="$1" actual="$2" msg="${3:-}"
  if [[ "$expected" == "$actual" ]]; then
    log_pass "${msg:-eq: '$expected'}"
  else
    log_fail "${msg:-eq}: expected='$expected' actual='$actual'"
  fi
}

# assert_contains <substring> <string> <message>
assert_contains() {
  local substr="$1" str="$2" msg="${3:-}"
  if [[ "$str" == *"$substr"* ]]; then
    log_pass "${msg:-contains: '$substr'}"
  else
    log_fail "${msg:-contains}: '$substr' not found in '$str'"
  fi
}

# assert_not_contains <substring> <string> <message>
assert_not_contains() {
  local substr="$1" str="$2" msg="${3:-}"
  if [[ "$str" != *"$substr"* ]]; then
    log_pass "${msg:-not_contains: '$substr'}"
  else
    log_fail "${msg:-not_contains}: '$substr' unexpectedly found in '$str'"
  fi
}

# assert_empty <value> <message>
assert_empty() {
  local val="$1" msg="${2:-}"
  if [[ -z "$val" ]]; then
    log_pass "${msg:-empty}"
  else
    log_fail "${msg:-empty}: expected empty, got '$val'"
  fi
}

# assert_not_empty <value> <message>
assert_not_empty() {
  local val="$1" msg="${2:-}"
  if [[ -n "$val" ]]; then
    log_pass "${msg:-not_empty}"
  else
    log_fail "${msg:-not_empty}: expected non-empty value"
  fi
}

# wait_for <timeout_seconds> <description> -- <command...>
# Polls every 5 seconds until the command exits 0 or timeout.
wait_for() {
  local timeout="$1" desc="$2"
  shift 2
  # consume the '--' separator if present
  if [[ "${1:-}" == "--" ]]; then shift; fi

  local elapsed=0
  while ! "$@" &>/dev/null; do
    if (( elapsed >= timeout )); then
      log_fail "Timeout after ${timeout}s waiting for: $desc"
      return 1
    fi
    sleep 5
    elapsed=$((elapsed + 5))
  done
  log_info "Ready after ${elapsed}s: $desc"
}

# wait_for_pod <namespace> <label_selector> <timeout_seconds>
wait_for_pod() {
  local ns="$1" selector="$2" timeout="${3:-120}"
  wait_for "$timeout" "pod ($selector) in $ns" -- \
    kubectl get pod -n "$ns" -l "$selector" --no-headers 2>/dev/null | grep -q Running
}

# wait_for_rollout <namespace> <deployment> <timeout_seconds>
wait_for_rollout() {
  local ns="$1" deploy="$2" timeout="${3:-120}"
  kubectl rollout status deployment/"$deploy" -n "$ns" --timeout="${timeout}s"
}

# print_summary — call at the end of run.sh
print_summary() {
  echo ""
  echo "================================"
  echo -e "Results: ${GREEN}${PASS_COUNT} passed${NC}, ${RED}${FAIL_COUNT} failed${NC}"
  echo "================================"
  if (( FAIL_COUNT > 0 )); then
    exit 1
  fi
}
