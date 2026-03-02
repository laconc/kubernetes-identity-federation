#!/usr/bin/env bash
# Test 07: A pod can access S3 objects in LocalStack using KIF-issued credentials.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$SCRIPT_DIR/lib.sh"

MANIFESTS="$SCRIPT_DIR/manifests"

echo "Applying S3 test job..."
kubectl apply -f "$MANIFESTS/test-job-s3.yaml"

echo "Waiting for test-s3 job to complete (up to 180s)..."
if kubectl wait --for=condition=Complete job/test-s3 -n apps --timeout=180s; then
  log_pass "job/test-s3 completed successfully"
else
  log_fail "job/test-s3 did not complete in time"
  kubectl logs -n apps job/test-s3 || true
  print_summary
  exit 1
fi

LOGS=$(kubectl logs -n apps job/test-s3)
assert_contains "kif-e2e-ok" "$LOGS" "s3: object content matches expected value"

print_summary
