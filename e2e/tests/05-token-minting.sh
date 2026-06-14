#!/usr/bin/env bash
# Test 05: The agent sidecar mints an OIDC token and writes it to the shared volume.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$SCRIPT_DIR/lib.sh"

ISSUER_URL="https://kif-issuer.kif.svc.cluster.local:5002"

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
PADDING=$(printf '%*s' "$PAD" '' | tr ' ' '=')
PAYLOAD=$(echo "${PAYLOAD_B64}${PADDING}" | tr '_-' '/+' | base64 -d 2>/dev/null || true)

assert_not_empty "$PAYLOAD" "token: JWT payload decoded"

ISS=$(echo "$PAYLOAD" | grep -o '"iss":"[^"]*"' | cut -d'"' -f4 || true)
assert_eq "$ISSUER_URL" "$ISS" "token: iss claim"

SUB=$(echo "$PAYLOAD" | grep -o '"sub":"[^"]*"' | cut -d'"' -f4 || true)
assert_eq "system:serviceaccount:apps:test-app" "$SUB" "token: sub claim"

# aud can be a string or array; check it contains sts.amazonaws.com
assert_contains "sts.amazonaws.com" "$PAYLOAD" "token: aud contains sts.amazonaws.com"

# ── Signing key is published in the issuer JWKS ────────────────────────────
# A relying party (e.g. AWS STS) fetches the issuer JWKS and verifies the token
# signature against the key named by the header `kid`. Full verification needs a
# JWT verifier; matching the header `kid` against the served JWKS is a cheap
# proxy that catches the most likely failure: the issuer publishing a different
# key than the one kif-federation signs with.
HEADER_B64=$(echo "$TOKEN" | cut -d. -f1)
HPAD=$(( (4 - ${#HEADER_B64} % 4) % 4 ))
HPADDING=$(printf '%*s' "$HPAD" '' | tr ' ' '=')
HEADER=$(echo "${HEADER_B64}${HPADDING}" | tr '_-' '/+' | base64 -d 2>/dev/null || true)
KID=$(echo "$HEADER" | grep -o '"kid":"[^"]*"' | cut -d'"' -f4 || true)
assert_not_empty "$KID" "token: header has a kid"

echo "Fetching issuer JWKS to confirm the signing key is published..."
JWKS=$(kubectl run "kif-e2e-jwks-$$" --rm -i --restart=Never --quiet \
  --image=curlimages/curl:8.11.0 -n default --command -- \
  curl -sf "http://kif-issuer.kif.svc.cluster.local:5002/jwks.json" 2>/dev/null || true)
assert_contains "\"$KID\"" "$JWKS" "token: signing kid is published in issuer JWKS"

print_summary
