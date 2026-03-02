#!/usr/bin/env bash
# Test 09: With admissionFailureMode=Fail, a pod referencing a CRB with no
# providers is rejected by the webhook.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$SCRIPT_DIR/lib.sh"

# ── Create SA and an invalid CRB (no providers configured) ────────────────
echo "Creating bad-app SA and empty-provider CRB..."
kubectl apply -f - <<'EOF'
apiVersion: v1
kind: ServiceAccount
metadata:
  name: bad-app
  namespace: apps
EOF

# A CRB with providers: {} means no aws/azure/gcp configured.
# The webhook should reject pods using this SA because it cannot determine
# what credentials to inject, and admissionFailureMode=Fail is active.
kubectl apply -f - <<'EOF'
apiVersion: 64f.dev/v1alpha1
kind: CloudRoleBinding
metadata:
  name: bad-app
  namespace: apps
spec:
  subject:
    serviceAccountName: bad-app
  providers: {}
EOF

echo "Waiting for bad-app RCRB to be reconciled (up to 30s)..."
wait_for 30 "bad-app ResolvedCloudRoleBinding created" -- \
  kubectl get resolvedcloudrolebinding bad-app -n apps
log_pass "CRB reconciled: bad-app RCRB exists"

# ── Attempt to create a pod using bad-app SA ──────────────────────────────
echo "Attempting to create a pod with the bad-app SA (expecting rejection)..."
TMPFILE=$(mktemp)
EXIT_CODE=0
kubectl apply -f - 2>"$TMPFILE" <<'EOF' || EXIT_CODE=$?
apiVersion: v1
kind: Pod
metadata:
  name: bad-pod
  namespace: apps
spec:
  serviceAccountName: bad-app
  restartPolicy: Never
  containers:
    - name: sleep
      image: busybox:1.36
      command: ["sh", "-c", "sleep 10"]
EOF

ERROR_MSG=$(cat "$TMPFILE")
rm -f "$TMPFILE"

if (( EXIT_CODE != 0 )); then
  log_pass "rejection: kubectl apply returned non-zero (pod rejected)"
  log_info "admission error: $ERROR_MSG"
  # The error message should reference the webhook or an admission denial
  if echo "$ERROR_MSG" | grep -qiE "admission|webhook|denied|rejected|error"; then
    log_pass "rejection: error message contains admission-related reason"
  else
    log_fail "rejection: unexpected error message: $ERROR_MSG"
  fi
else
  log_fail "rejection: pod was unexpectedly admitted (exit code 0)"
  kubectl delete pod bad-pod -n apps --ignore-not-found=true &>/dev/null || true
fi

# ── Cleanup ────────────────────────────────────────────────────────────────
kubectl delete pod bad-pod -n apps --ignore-not-found=true &>/dev/null || true
kubectl delete cloudrolebinding bad-app -n apps --ignore-not-found=true &>/dev/null || true
kubectl delete sa bad-app -n apps --ignore-not-found=true &>/dev/null || true

print_summary
