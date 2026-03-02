#!/usr/bin/env bash
# Test 02: kif-issuer serves a valid OIDC discovery document and non-empty JWKS.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$SCRIPT_DIR/lib.sh"

ISSUER_URL="http://kif-issuer.kif.svc.cluster.local:5002"

# Use a temporary curl pod inside the cluster to hit the in-cluster issuer URL.
CURL_POD="kif-e2e-curl"
kubectl delete pod "$CURL_POD" -n default --ignore-not-found=true --wait=true 2>/dev/null || true
kubectl run "$CURL_POD" \
  --image=curlimages/curl:8.11.0 \
  --restart=Never \
  --command -- sh -c "sleep 120" \
  -n default

echo "Waiting for curl pod..."
kubectl wait pod/"$CURL_POD" -n default --for=condition=Ready --timeout=60s

run_curl() {
  kubectl exec -n default "$CURL_POD" -- \
    curl -sf "$@"
}

# ── OIDC discovery ─────────────────────────────────────────────────────────
echo "Fetching OIDC discovery document..."
DISCOVERY=$(run_curl "${ISSUER_URL}/.well-known/openid-configuration")

ISSUER_FIELD=$(echo "$DISCOVERY" | grep -o '"issuer":"[^"]*"' | cut -d'"' -f4 || true)
assert_eq "$ISSUER_URL" "$ISSUER_FIELD" "discovery: issuer field"

assert_contains "jwks_uri" "$DISCOVERY" "discovery: contains jwks_uri"
assert_contains "id_token_signing_alg_values_supported" "$DISCOVERY" "discovery: contains signing algs"

# ── JWKS ───────────────────────────────────────────────────────────────────
echo "Fetching JWKS..."
JWKS=$(run_curl "${ISSUER_URL}/jwks.json")

assert_contains '"keys"' "$JWKS" "jwks: contains keys array"
assert_contains '"RS256"' "$JWKS" "jwks: alg is RS256"

# Check at least one key exists: look for a non-empty "n" (modulus) field
KEY_COUNT=$(echo "$JWKS" | grep -o '"kty"' | wc -l | tr -d ' ')
if (( KEY_COUNT >= 1 )); then
  log_pass "jwks: at least one key present (count=$KEY_COUNT)"
else
  log_fail "jwks: no keys found"
fi

# ── Cleanup ────────────────────────────────────────────────────────────────
kubectl delete pod "$CURL_POD" -n default --ignore-not-found=true &>/dev/null || true

print_summary
