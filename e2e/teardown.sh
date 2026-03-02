#!/usr/bin/env bash
# Tear down the e2e kind cluster. Always runs, even on test failure.
set -euo pipefail

CLUSTER_NAME="${CLUSTER_NAME:-kif-e2e}"

echo "[teardown] Deleting kind cluster: $CLUSTER_NAME"
kind delete cluster --name "$CLUSTER_NAME" 2>/dev/null || true
echo "[teardown] Done."
