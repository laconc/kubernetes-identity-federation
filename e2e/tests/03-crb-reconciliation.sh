#!/usr/bin/env bash
# Test 03: Creating a CloudRoleBinding triggers reconciliation into a ResolvedCloudRoleBinding.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$SCRIPT_DIR/lib.sh"

MANIFESTS="$SCRIPT_DIR/manifests"

echo "Applying test ServiceAccount and CloudRoleBinding..."
kubectl apply -f "$MANIFESTS/test-serviceaccount.yaml"
kubectl apply -f "$MANIFESTS/test-cloudrolebinding.yaml"

echo "Waiting for ResolvedCloudRoleBinding to be created (up to 60s)..."
wait_for 60 "resolvedcloudrolebinding/test-app in apps" -- \
  kubectl get resolvedcloudrolebinding/test-app -n apps

# Fetch the RCRB and inspect
RCRB=$(kubectl get resolvedcloudrolebinding/test-app -n apps -o json)

SA_NAME=$(echo "$RCRB" | grep -o '"serviceAccountName":"[^"]*"' | head -1 | cut -d'"' -f4 || true)
assert_eq "test-app" "$SA_NAME" "rcrb: subject.serviceAccountName"

ROLE_ARN=$(echo "$RCRB" | grep -o '"roleArn":"[^"]*"' | head -1 | cut -d'"' -f4 || true)
assert_eq "arn:aws:iam::000000000000:role/test-role" "$ROLE_ARN" "rcrb: aws.roleArn"

PROVIDERS_SUMMARY=$(echo "$RCRB" | grep -o '"providers":"[^"]*"' | head -1 | cut -d'"' -f4 || true)
assert_contains "aws" "$PROVIDERS_SUMMARY" "rcrb: providers summary contains aws"

print_summary
