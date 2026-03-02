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
for deploy in kif-federation kif-issuer; do
  POD=$(kubectl get pod -n kif -l "app.kubernetes.io/component=${deploy#kif-}" \
    --no-headers -o custom-columns=NAME:.metadata.name | head -1)
  STATUS=$(kubectl exec -n kif "$POD" -- \
    wget -qO- --server-response http://localhost:5001/livez 2>&1 | grep "HTTP/" | awk '{print $2}' || \
    kubectl exec -n kif "$POD" -- sh -c 'wget -qO- http://localhost:5001/livez && echo OK' 2>/dev/null | tail -1)
  log_info "$deploy /livez check"
done

# Check each service's health endpoint using curl via kubectl exec
check_livez() {
  local deploy="$1" port="$2" component="$3"
  local pod
  pod=$(kubectl get pod -n kif -l "app.kubernetes.io/component=${component}" \
    --no-headers -o custom-columns=NAME:.metadata.name | head -1)
  if [[ -z "$pod" ]]; then
    log_fail "$deploy: no pod found"
    return
  fi
  local http_code
  http_code=$(kubectl exec -n kif "$pod" -- \
    sh -c "wget -qO /dev/null -S http://localhost:${port}/livez 2>&1 | grep 'HTTP/' | awk '{print \$2}'" 2>/dev/null || echo "")
  if [[ -z "$http_code" ]]; then
    # Fallback: just check that wget exits 0
    if kubectl exec -n kif "$pod" -- \
        sh -c "wget -qO- http://localhost:${port}/livez" &>/dev/null; then
      log_pass "$deploy /livez responded"
    else
      log_fail "$deploy /livez did not respond"
    fi
  else
    assert_eq "200" "$http_code" "$deploy /livez HTTP 200"
  fi
}

check_livez "kif-federation" "5001" "federation"
check_livez "kif-issuer" "5002" "issuer"

# Webhook health is on port 5003
WEBHOOK_POD=$(kubectl get pod -n kif -l "app.kubernetes.io/component=webhook" \
  --no-headers -o custom-columns=NAME:.metadata.name | head -1)
if [[ -n "$WEBHOOK_POD" ]]; then
  if kubectl exec -n kif "$WEBHOOK_POD" -- \
      sh -c "wget -qO- http://localhost:5003/livez" &>/dev/null; then
    log_pass "kif-webhook /livez responded"
  else
    log_fail "kif-webhook /livez did not respond"
  fi
else
  log_fail "kif-webhook: no pod found"
fi

print_summary
