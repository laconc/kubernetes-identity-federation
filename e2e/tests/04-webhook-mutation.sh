#!/usr/bin/env bash
# Test 04: A pod whose SA has a CRB gets the kif-agent sidecar injected.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$SCRIPT_DIR/lib.sh"

MANIFESTS="$SCRIPT_DIR/manifests"

echo "Applying test pod..."
kubectl apply -f "$MANIFESTS/test-pod.yaml"

echo "Waiting for test-pod to be scheduled (up to 60s)..."
wait_for 60 "test-pod scheduled" -- \
  kubectl get pod test-pod -n apps --no-headers 2>/dev/null | grep -qv "Pending"

POD_JSON=$(kubectl get pod test-pod -n apps -o json)

# ── Init containers ────────────────────────────────────────────────────────
INIT_NAME=$(kubectl get pod test-pod -n apps \
  -o jsonpath='{.spec.initContainers[0].name}' || true)
assert_eq "kif-agent" "$INIT_NAME" "webhook: initContainers[0].name == kif-agent"

# ── Volumes ────────────────────────────────────────────────────────────────
assert_contains '"kif-sa-token"' "$POD_JSON" "webhook: volume kif-sa-token present"
assert_contains '"kif-aws"' "$POD_JSON" "webhook: volume kif-aws present"

# ── Env vars on main container ─────────────────────────────────────────────
assert_contains '"AWS_WEB_IDENTITY_TOKEN_FILE"' "$POD_JSON" \
  "webhook: AWS_WEB_IDENTITY_TOKEN_FILE injected"
assert_contains '"AWS_ROLE_ARN"' "$POD_JSON" \
  "webhook: AWS_ROLE_ARN injected"
assert_contains '"arn:aws:iam::000000000000:role/test-role"' "$POD_JSON" \
  "webhook: AWS_ROLE_ARN value"

# ── automountServiceAccountToken ──────────────────────────────────────────
AUTOMOUNT=$(echo "$POD_JSON" | \
  grep -o '"automountServiceAccountToken":[^,}]*' | head -1 | \
  cut -d: -f2 | tr -d ' ' || true)
assert_eq "false" "$AUTOMOUNT" "webhook: automountServiceAccountToken=false"

print_summary
