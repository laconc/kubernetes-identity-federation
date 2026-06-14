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

echo "Waiting for RCRB test-app to be Ready (up to 60s)..."
kubectl wait resolvedcloudrolebinding/test-app -n apps \
  --for=condition=Ready --timeout=60s
log_pass "rcrb: test-app is Ready"

# Fetch and inspect RCRB fields via jsonpath (robust against JSON formatting).
rcrb_field() { kubectl get resolvedcloudrolebinding/test-app -n apps -o jsonpath="$1"; }

SA_NAME=$(rcrb_field '{.spec.subject.serviceAccountName}')
assert_eq "test-app" "$SA_NAME" "rcrb: subject.serviceAccountName"

ROLE_ARN=$(rcrb_field '{.spec.providers.aws.roleArn}')
assert_eq "arn:aws:iam::000000000000:role/test-role" "$ROLE_ARN" "rcrb: aws.roleArn"

PROVIDERS_SUMMARY=$(rcrb_field '{.status.providers}')
assert_contains "aws" "$PROVIDERS_SUMMARY" "rcrb: providers summary contains aws"

print_summary
