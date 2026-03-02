#!/usr/bin/env bash
# Test 01: All KIF deployments are Running and /livez endpoints respond.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$SCRIPT_DIR/lib.sh"

echo "Checking deployment rollout status..."
for deploy in kif-federation kif-issuer kif-webhook; do
  kubectl rollout status deployment/"$deploy" -n kif --timeout=60s
  log_pass "$deploy rollout complete"
done

echo "Checking /livez endpoints..."
check_livez() {
  local target="$1" local_port="$2" remote_port="$3"
  kubectl port-forward -n kif "${target}" "${local_port}:${remote_port}" &>/dev/null &
  local pf_pid=$!
  sleep 2
  local http_code
  http_code=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:${local_port}/livez") || true
  kill "$pf_pid" 2>/dev/null || true
  wait "$pf_pid" 2>/dev/null || true
  assert_eq "200" "$http_code" "${target} /livez HTTP 200"
}

check_livez "svc/kif-federation" 15001 5001
check_livez "svc/kif-issuer"     15002 5002
check_livez "deploy/kif-webhook" 15003 5003

print_summary
