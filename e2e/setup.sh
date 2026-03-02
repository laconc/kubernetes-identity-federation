#!/usr/bin/env bash
# Full e2e environment setup:
#   1. Create kind cluster
#   2. Install cert-manager
#   3. Build + load Docker images
#   4. Deploy LocalStack
#   5. Create apps namespace
#   6. Helm install kif
#   7. Configure LocalStack IAM + S3
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

CLUSTER_NAME="${CLUSTER_NAME:-kif-e2e}"
IMAGE_TAG="${IMAGE_TAG:-kif-e2e}"
CERT_MANAGER_VERSION="${CERT_MANAGER_VERSION:-v1.19.4}"
LOCALSTACK_ENDPOINT="http://localhost:4566"

# ── 1. Create kind cluster ─────────────────────────────────────────────────
echo "[setup] Creating kind cluster: $CLUSTER_NAME"
if kind get clusters 2>/dev/null | grep -q "^${CLUSTER_NAME}$"; then
  echo "[setup] Cluster already exists, skipping."
else
  kind create cluster --name "$CLUSTER_NAME" \
    --config "$SCRIPT_DIR/kind-config.yaml"
fi

# ── 2. Install cert-manager ────────────────────────────────────────────────
echo "[setup] Installing cert-manager $CERT_MANAGER_VERSION"
kubectl apply -f \
  "https://github.com/cert-manager/cert-manager/releases/download/${CERT_MANAGER_VERSION}/cert-manager.yaml"
kubectl wait --for=condition=Available deployment --all \
  -n cert-manager --timeout=120s

# ── 3. Build images + load into kind ──────────────────────────────────────
echo "[setup] Building images (tag: $IMAGE_TAG)"
make -C "$SCRIPT_DIR/.." build-images IMAGE_TAG="$IMAGE_TAG" IMAGE_PREFIX=localhost

echo "[setup] Loading images into cluster $CLUSTER_NAME"
for svc in kif-agent kif-federation kif-issuer kif-webhook; do
  kind load docker-image "localhost/${svc}:${IMAGE_TAG}" --name "$CLUSTER_NAME"
done

# If kif deployments already exist, restart them to pick up any new images
kubectl rollout restart deployment/kif-federation deployment/kif-issuer deployment/kif-webhook \
  -n kif 2>/dev/null || true

# ── 4. Deploy LocalStack ───────────────────────────────────────────────────
echo "[setup] Deploying LocalStack"
kubectl apply -f "$SCRIPT_DIR/manifests/localstack.yaml"
kubectl rollout status deployment/localstack -n default --timeout=120s

# ── 5. Create apps namespace ───────────────────────────────────────────────
echo "[setup] Creating apps namespace"
kubectl apply -f "$SCRIPT_DIR/manifests/apps-namespace.yaml"

# ── 6. Helm install kif ────────────────────────────────────────────────────
echo "[setup] Installing kif via Helm"
helm upgrade --install kif "$SCRIPT_DIR/../deploy/charts/kif" \
  -n kif --create-namespace \
  --set image.repository=localhost \
  --set image.tag="$IMAGE_TAG" \
  --set image.pullPolicy=Never \
  --set federation.config.issuerUrl=http://kif-issuer.kif.svc.cluster.local:5002

echo "[setup] Waiting for kif deployments"
for deploy in kif-federation kif-issuer kif-webhook; do
  kubectl rollout status deployment/"$deploy" -n kif --timeout=120s
done

# ── 7. Configure LocalStack IAM + S3 ──────────────────────────────────────
echo "[setup] Setting up LocalStack port-forward"
kubectl port-forward -n default svc/localstack 4566:4566 &
PF_PID=$!
trap 'kill $PF_PID 2>/dev/null || true' EXIT

# Wait for port-forward to be ready
for _ in $(seq 1 20); do
  if curl -sf "${LOCALSTACK_ENDPOINT}/_localstack/health" &>/dev/null; then
    break
  fi
  sleep 3
done
echo "[setup] LocalStack is reachable"

export AWS_ACCESS_KEY_ID=test
export AWS_SECRET_ACCESS_KEY=test
export AWS_DEFAULT_REGION=us-east-1
export AWS_ENDPOINT_URL="$LOCALSTACK_ENDPOINT"
export AWS_PAGER=""

ISSUER_URL="http://kif-issuer.kif.svc.cluster.local:5002"
ROLE_NAME="test-role"
SA_SUBJECT="system:serviceaccount:apps:test-app"
BUCKET="kif-e2e-bucket"

echo "[setup] Creating OIDC provider in LocalStack"
aws iam create-open-id-connect-provider \
  --url "$ISSUER_URL" \
  --client-id-list "sts.amazonaws.com" \
  --thumbprint-list "0000000000000000000000000000000000000000" \
  --endpoint-url "$LOCALSTACK_ENDPOINT" || true

echo "[setup] Creating IAM role: $ROLE_NAME"
TRUST_POLICY=$(cat <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "Federated": "arn:aws:iam::000000000000:oidc-provider/kif-issuer.kif.svc.cluster.local:5002"
      },
      "Action": "sts:AssumeRoleWithWebIdentity",
      "Condition": {
        "StringEquals": {
          "kif-issuer.kif.svc.cluster.local:5002:sub": "$SA_SUBJECT",
          "kif-issuer.kif.svc.cluster.local:5002:aud": "sts.amazonaws.com"
        }
      }
    }
  ]
}
EOF
)
aws iam create-role \
  --role-name "$ROLE_NAME" \
  --assume-role-policy-document "$TRUST_POLICY" \
  --endpoint-url "$LOCALSTACK_ENDPOINT" || true

echo "[setup] Attaching inline policy to $ROLE_NAME"
POLICY=$(cat <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": ["sts:GetCallerIdentity", "sts:AssumeRoleWithWebIdentity"],
      "Resource": "*"
    },
    {
      "Effect": "Allow",
      "Action": ["s3:GetObject"],
      "Resource": "arn:aws:s3:::${BUCKET}/*"
    }
  ]
}
EOF
)
aws iam put-role-policy \
  --role-name "$ROLE_NAME" \
  --policy-name kif-e2e-policy \
  --policy-document "$POLICY" \
  --endpoint-url "$LOCALSTACK_ENDPOINT" || true

echo "[setup] Creating S3 bucket and uploading test object"
aws s3 mb "s3://${BUCKET}" --endpoint-url "$LOCALSTACK_ENDPOINT" || true
echo "kif-e2e-ok" | aws s3 cp - "s3://${BUCKET}/verify.txt" \
  --endpoint-url "$LOCALSTACK_ENDPOINT"

echo "[setup] Stopping port-forward"
kill "$PF_PID" 2>/dev/null || true
trap - EXIT

echo "[setup] Setup complete."
