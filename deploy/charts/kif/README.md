# kubernetes-identity-federation Helm Chart

This chart deploys [kubernetes-identity-federation](https://github.com/laconc/kubernetes-identity-federation) — a system that allows Kubernetes workloads to authenticate and access AWS, Azure, and GCP resources using OIDC federation, without long-lived cloud credentials.

## Prerequisites

- Kubernetes >= 1.29 (native sidecar containers via `initContainers[].restartPolicy: Always`, GA in 1.29)
- [cert-manager](https://cert-manager.io/docs/installation/) >= 1.0 installed in the cluster (when using the default TLS mode)
- A publicly accessible URL for the issuer service (cloud providers call OIDC discovery endpoints to verify tokens)

## Installation

### Default (cert-manager already installed)

cert-manager is used to automatically provision a self-signed TLS certificate for the webhook. This requires cert-manager to already be present in the cluster.

```bash
helm install kif deploy/charts/kif \
  --namespace kif-system --create-namespace \
  --set federation.config.issuerUrl=https://oidc.example.com
```

### Manual TLS (no cert-manager)

If you prefer to not rely on cert-manager.

> **Why is `caBundle` required?** The `MutatingWebhookConfiguration` resource needs to trust the webhook's TLS certificate. Since Helm cannot read cluster secrets at install time, you must supply the base64-encoded CA certificate as a value. For a self-signed certificate, the CA cert is the certificate itself.

```bash
# 1. Generate a self-signed certificate
openssl req -x509 -newkey rsa:4096 -keyout tls.key -out tls.crt -days 365 -nodes \
  -subj "/CN=kif-webhook" \
  -addext "subjectAltName=DNS:kif-webhook,DNS:kif-webhook.kif-system.svc,DNS:kif-webhook.kif-system.svc.cluster.local"

# 2. Create the TLS secret
kubectl create secret tls kif-webhook-tls --cert=tls.crt --key=tls.key -n kif-system

# 3. Install the chart (caBundle = base64 of the CA cert; for self-signed, that is the cert itself)
helm install kif deploy/charts/kif \
  --namespace kif-system --create-namespace \
  --set federation.config.issuerUrl=https://oidc.example.com \
  --set webhook.tls.certManager.enabled=false \
  --set webhook.tls.caBundle="$(base64 < tls.crt | tr -d '\n')"
```

## Post-install notes

- The **issuer** pod will remain not-ready until the **federation** service creates the `kif-jwks` secret
- The **issuer** service must be exposed publicly so cloud providers can reach the OIDC discovery and JWKS endpoints

## Values

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `crds.install` | bool | `true` | Install CRD resources as part of this chart |
| `crds.keep` | bool | `true` | Prevent CRDs from being deleted when the release is uninstalled (`helm.sh/resource-policy: keep`) |
| `image.repository` | string | `quay.io/laconc` | Image registry and repository prefix |
| `image.tag` | string | `latest` | Image tag applied to all services unless overridden |
| `image.pullPolicy` | string | `IfNotPresent` | Image pull policy |
| `podSecurityContext` | object | `{runAsNonRoot: true, runAsUser: 2000, runAsGroup: 2000, seccompProfile: {type: RuntimeDefault}}` | Pod-level security context applied to all Deployments. Per-service `podSecurityContext` replaces this entirely when non-empty. |
| `containerSecurityContext` | object | `{allowPrivilegeEscalation: false, readOnlyRootFilesystem: true, capabilities: {drop: [ALL]}}` | Container-level security context applied to all containers. Per-service `containerSecurityContext` replaces this entirely when non-empty. |
| `federation.replicaCount` | int | `1` | Number of federation replicas |
| `federation.image` | object | `{}` | Per-service image overrides (repository, tag, pullPolicy) |
| `federation.podAnnotations` | object | `{}` | Annotations added to each federation Pod |
| `federation.podLabels` | object | `{}` | Extra labels added to each federation Pod |
| `federation.podSecurityContext` | object | `{}` | Overrides the global `podSecurityContext` for federation Pods. When non-empty, replaces the global entirely. |
| `federation.containerSecurityContext` | object | `{}` | Overrides the global `containerSecurityContext` for the federation container. When non-empty, replaces the global entirely. |
| `federation.serviceAccount.name` | string | `kif-federation` | ServiceAccount name for the federation service |
| `federation.service.type` | string | `ClusterIP` | Service type |
| `federation.service.port` | int | `5001` | Service port |
| `federation.config.issuerUrl` | string | `""` | **REQUIRED.** Public OIDC issuer URL. Must be stable and publicly reachable. |
| `federation.config.port` | int | `5001` | Container port |
| `federation.config.signingSecretName` | string | `kif-signing` | Name of the Secret for the signing key pair |
| `federation.config.jwksSecretName` | string | `kif-jwks` | Name of the Secret for the public JWKS. Also used by the issuer. |
| `issuer.replicaCount` | int | `1` | Number of issuer replicas |
| `issuer.image` | object | `{}` | Per-service image overrides |
| `issuer.podAnnotations` | object | `{}` | Annotations added to each issuer Pod |
| `issuer.podLabels` | object | `{}` | Extra labels added to each issuer Pod |
| `issuer.podSecurityContext` | object | `{}` | Overrides the global `podSecurityContext` for issuer Pods. When non-empty, replaces the global entirely. |
| `issuer.containerSecurityContext` | object | `{}` | Overrides the global `containerSecurityContext` for the issuer container. When non-empty, replaces the global entirely. |
| `issuer.serviceAccount.name` | string | `kif-issuer` | ServiceAccount name for the issuer service |
| `issuer.service.type` | string | `ClusterIP` | Service type. Expose via ingress or LoadBalancer for public access. |
| `issuer.service.port` | int | `5002` | Service port |
| `issuer.config.port` | int | `5002` | Container port |
| `webhook.replicaCount` | int | `1` | Number of webhook replicas |
| `webhook.image` | object | `{}` | Per-service image overrides |
| `webhook.podAnnotations` | object | `{}` | Annotations added to each webhook Pod |
| `webhook.podLabels` | object | `{}` | Extra labels added to each webhook Pod |
| `webhook.podSecurityContext` | object | `{}` | Overrides the global `podSecurityContext` for webhook Pods. When non-empty, replaces the global entirely. |
| `webhook.containerSecurityContext` | object | `{}` | Overrides the global `containerSecurityContext` for the webhook container. When non-empty, replaces the global entirely. |
| `webhook.serviceAccount.name` | string | `kif-webhook` | ServiceAccount name for the webhook service |
| `webhook.service.port` | int | `443` | Port the webhook Service exposes to the API server |
| `webhook.config.admissionFailureMode` | string | `Fail` | Admission failure mode: `Fail` (deny pod) or `Skip` (allow with warning) |
| `webhook.config.reconcileWorkers` | int | `4` | Number of reconciliation workers |
| `webhook.config.port` | int | `9443` | HTTPS port for the admission webhook server |
| `webhook.config.healthPort` | int | `5003` | HTTP port for liveness/startup probes |
| `webhook.tls.secretName` | string | `kif-webhook-tls` | Name of the Secret containing `tls.crt` and `tls.key` |
| `webhook.tls.certManager.enabled` | bool | `true` | Use cert-manager to provision the webhook TLS certificate |
| `webhook.tls.caBundle` | string | `""` | Base64-encoded CA certificate for the `MutatingWebhookConfiguration`. Required when `certManager.enabled=false`. |
| `agent.image` | object | `{}` | Per-service image overrides for the agent sidecar |
| `agent.config.port` | int | `5004` | Port the agent sidecar listens on |
| `agent.config.refreshSkewSeconds` | int | `300` | Seconds before token expiry to start refreshing |
| `agent.config.minRefreshSeconds` | int | `30` | Minimum interval between refreshes |
| `agent.config.maxJitterSeconds` | int | `60` | Maximum random jitter added to refresh interval |
