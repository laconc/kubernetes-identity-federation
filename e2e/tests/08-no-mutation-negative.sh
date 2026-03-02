#!/usr/bin/env bash
# Test 08: Pods using a SA without a CloudRoleBinding are left unmodified.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$SCRIPT_DIR/lib.sh"

MANIFESTS="$SCRIPT_DIR/manifests"

echo "Applying unmutated pod (default SA, no CRB)..."
kubectl apply -f "$MANIFESTS/unmutated-pod.yaml"

echo "Waiting for unmutated-pod to be scheduled (up to 60s)..."
wait_for 60 "unmutated-pod scheduled" -- \
  kubectl get pod unmutated-pod -n apps --no-headers 2>/dev/null | grep -q .

POD_JSON=$(kubectl get pod unmutated-pod -n apps -o json)

# ── No kif-agent init container ────────────────────────────────────────────
INIT_CONTAINERS=$(echo "$POD_JSON" | \
  grep -o '"initContainers":\[[^]]*\]' | head -1 || true)

if [[ -z "$INIT_CONTAINERS" ]] || \
   ! echo "$INIT_CONTAINERS" | grep -q '"kif-agent"'; then
  log_pass "no-mutation: kif-agent not in initContainers"
else
  log_fail "no-mutation: kif-agent unexpectedly found in initContainers"
fi

# ── No kif-aws volume ─────────────────────────────────────────────────────
assert_not_contains '"kif-aws"' "$POD_JSON" "no-mutation: kif-aws volume not present"

# ── No AWS env vars ───────────────────────────────────────────────────────
assert_not_contains '"AWS_WEB_IDENTITY_TOKEN_FILE"' "$POD_JSON" \
  "no-mutation: AWS_WEB_IDENTITY_TOKEN_FILE not injected"

print_summary
