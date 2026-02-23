# kubernetes-identity-federation

**_Note: This project is under active development. Features and behavior may change frequently._**

Current cloud support: AWS (Azure & GCP planned)

## Overview

The motivation for this project was to provide [IRSA](https://docs.aws.amazon.com/eks/latest/userguide/iam-roles-for-service-accounts.html)-like functionality for Kubernetes clusters running anywhere,
including bare metal, and we wanted to support the three major cloud providers: AWS, Azure, and GCP.

Much like IRSA, this project allows Kubernetes workloads (through ServiceAccounts) to auth and access AWS, Azure, and
GCP resources using OIDC federation.

Traditionally, teams resort to creating long-lived cloud credentials and storing them in Kubernetes secrets, which is
just not great. This is hardly the first attempt at solving this problem, but we wanted to provide a solution that was
available to any Kubernetes cluster, provided a great experience for admins and developers, and supported multiple clouds.

## Architecture

To make this work, we have a number of components that must be deployed in your cluster. We provide a [Helm chart](helm)
to make this easier.

### Components

* CRD: `CloudRoleBinding` - Configuration objects to define the mapping between Kubernetes ServiceAccounts and cloud identities. More in the next section.
  * There can be multiple CloudRoleBindings referring to the same ServiceAccount, or a single CloudRoleBinding referring to multiple providers;
  the CRD configurations are merged at Pod admission time.
  * It's unsupported to have multiple CloudRoleBindings with conflicting configurations for the same ServiceAccount.

* `Issuer` service - A minimal OIDC issuer that serves only the two public endpoints required for cloud verification:
`/.well-known/openid-configuration` and `/jwks.json`
  * Given the infrequency of changes to those endpoints, we recommend running this service behind a CDN

* `Federation` service - The core of the system. This private service is responsible for:
  * Validating Kubernetes ServiceAccount tokens (through the TokenReview API)
  * Minting the cloud-specific OIDC tokens (one per provider)
  * Writing and rotating the private signing keys, and the public JWKS used for verification by the cloud providers

* `Agent` sidecar - A sidecar attached to Pods mounting relevant ServiceAccounts. The agent is responsible for:
  * Requesting provider tokens from the `federation` service
  * Refreshing tokens before they expire
  * Mounting the provider tokens in container volumes for the cloud provider SDKs and setting associated env vars

* `Webhook` service - A mutating admission webhook that:
  * Intercepts Pod creation events
  * Reads and validates new/updated CloudRoleBinding resources
  * Associates a Pod's ServiceAccount with the appropriate CloudRoleBinding(s), and performs validation on the merged configuration
  * Injects into the Pod spec:
    * The `agent` sidecar
    * A projected ServiceAccount token volume
    * The necessary cloud provider token volume mounts and env vars

*How it all fits together:*
* The `federation` service owns key lifecycle and token minting
* The `issuer` serves the public OIDC discovery + JWKS so cloud providers can verify the minted tokens
* The `webhook` service injects the right plumbing into Pods based on the `CloudRoleBindings`
* The `agent` sidecar handles fetching and refreshing tokens from the `federation` service and making them available to the containers

## Custom Resource - CloudRoleBinding

CloudRoleBinding binds a Kubernetes ServiceAccount to one or more cloud identities.

This example provides a small sample of the configuration options available. The full schema is available in the [CRD manifest](deploy/crd/cloudrolebinding.yaml).
```yaml
# Example: single CloudRoleBinding configuring all three providers
apiVersion: 64f.dev/v1alpha1
kind: CloudRoleBinding
metadata:
  name: example-multicloud
  namespace: app1
spec:
  subject:
    serviceAccountName: app1

  # Attributes are optional. They're included in the minted provider tokens for audit and ABAC use cases.
  attributes:

    # Includes attributes from the workload such as the pod name and namespace.
    # Defaults to true.
    includeProvenance: true

    # Arbitrary key-value pairs
    extra:
      team: platform
      app: invoice-processor

  providers:
    aws:
      roleArn: arn:aws:iam::123456789012:role/example
      region: us-west-2 # Optional default region for the AWS SDK
      stsRegionalEndpoints: true
      maxSessionDurationSeconds: 3600

    azure:
      tenantId: 00000000-0000-0000-0000-000000000000
      clientId: 11111111-1111-1111-1111-111111111111

    gcp:
      workloadIdentityProvider: projects/123456789/locations/global/workloadIdentityPools/pool/providers/provider
      serviceAccountEmail: my-sa@my-project.iam.gserviceaccount.com
      projectId: my-project
```

## Getting Started

With this system, developers are able to simplify cloud auth for their apps and cluster operators have a clear and auditable
mapping from Kubernetes ServiceAccounts to cloud identities, without needing to manage secrets or long-lived credentials.

### Installation

The recommended way to install is through our Helm chart.

### Developer (or platform team) responsibilities

  * Create and manage the CloudRoleBinding resources
  * Ensure the IAM roles have the appropriate trust relationships and permissions for the workloads

## Development

### Lint and run unit tests

```shell
make lint
make test
```

### Regenerate the CRD schema

After making changes to the CRD specs at [app/crates/kif-api/src/crd.rs](app/crates/kif-api/src/crd.rs), you can regenerate the CRD manifest with:

```shell
make crdgen
```
