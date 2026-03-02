#!/usr/bin/env bash
# Test 06: A pod can exchange its KIF token for AWS credentials via LocalStack STS.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$SCRIPT_DIR/lib.sh"

MANIFESTS="$SCRIPT_DIR/manifests"

echo "Applying STS test job..."
kubectl apply -f "$MANIFESTS/test-job-sts.yaml"

echo "Waiting for test-sts job to complete (up to 180s)..."
if kubectl wait --for=condition=Complete job/test-sts -n apps --timeout=180s; then
  log_pass "job/test-sts completed successfully"
else
  log_fail "job/test-sts did not complete in time"
  kubectl logs -n apps job/test-sts || true
  print_summary
  exit 1
fi

LOGS=$(kubectl logs -n apps job/test-sts)
assert_not_empty "$LOGS" "sts: job produced output"
assert_contains "test-role" "$LOGS" "sts: response ARN contains test-role"

print_summary
