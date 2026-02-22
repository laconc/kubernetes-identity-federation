# kubernetes-identity-federation

**_Note: This project is under active development. Features and behavior may change frequently. Currently only supports AWS._**

## Overview

The motivation for this project was to provide [IRSA](https://docs.aws.amazon.com/eks/latest/userguide/iam-roles-for-service-accounts.html)-like functionality for Kubernetes clusters running anywhere, including bare metal,
and we wanted to support the three major cloud providers: AWS, Azure, and GCP.

## Features

## Getting Started

## Architecture

## Custom Resource - CloudRoleBinding

Description.

Example:
```yaml
```

## Development

### Regenerate the CRD schema

After making changes to the CRD specs at [app/crates/api/src/crd.rs](app/crates/kif-api/src/crd.rs), you can regenerate the CRD manifest with:

```shell
make crdgen
```
