#!/usr/bin/env bash
# Test 05: The agent sidecar mints an OIDC token and writes it to the shared volume.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$SCRIPT_DIR/lib.sh"

ISSUER_URL="http://kif-issuer.kif.svc.cluster.local:5002"

echo "Waiting for test-pod to be Ready (up to 120s)..."
kubectl wait pod/test-pod -n apps --for=condition=Ready --timeout=120s

echo "Reading token from shared volume..."
TOKEN=$(kubectl exec -n apps test-pod -c sleep -- \
  sh -c 'cat /var/run/kif/aws/token 2>/dev/null || true')

assert_not_empty "$TOKEN" "token: file is non-empty"

# ── Decode JWT payload (base64url, no padding) ─────────────────────────────
# Extract the second segment (payload) and decode it.
PAYLOAD_B64=$(echo "$TOKEN" | cut -d. -f2)
# Pad base64url to a multiple of 4 characters for standard base64 decoding.
PAD=$(( (4 - ${#PAYLOAD_B64} % 4) % 4 ))
PADDING=$(printf '%0.s=' $(seq 1 "$PAD"))
PAYLOAD=$(echo "${PAYLOAD_B64}${PADDING}" | tr '_-' '/+' | base64 -d 2>/dev/null || true)

assert_not_empty "$PAYLOAD" "token: JWT payload decoded"

ISS=$(echo "$PAYLOAD" | grep -o '"iss":"[^"]*"' | cut -d'"' -f4 || true)
assert_eq "$ISSUER_URL" "$ISS" "token: iss claim"

SUB=$(echo "$PAYLOAD" | grep -o '"sub":"[^"]*"' | cut -d'"' -f4 || true)
assert_eq "system:serviceaccount:apps:test-app" "$SUB" "token: sub claim"

# aud can be a string or array; check it contains sts.amazonaws.com
assert_contains "sts.amazonaws.com" "$PAYLOAD" "token: aud contains sts.amazonaws.com"

print_summary
