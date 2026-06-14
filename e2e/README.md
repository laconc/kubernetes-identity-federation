# e2e tests

End-to-end tests that stand up a real `kind` cluster, install kif via Helm,
deploy LocalStack as a stand-in for AWS, and exercise the full flow.

## Running

```sh
make e2e            # setup + run + teardown
make e2e-setup      # create cluster, build/load images, install kif + LocalStack
./e2e/run.sh        # run the test suite against an existing setup
make e2e-teardown   # delete the kind cluster
```

Requires: `kind`, `kubectl`, `helm`, `docker` (or a Docker-compatible engine),
`aws`, `curl`.

## Tests

| Test | Covers |
|------|--------|
| 01-health | deployments roll out; `/livez` responds |
| 02-oidc-discovery | issuer serves a valid OIDC discovery doc + JWKS |
| 03-crb-reconciliation | a CloudRoleBinding reconciles into a Ready ResolvedCloudRoleBinding |
| 04-webhook-mutation | bound pods get the kif-agent sidecar + AWS env injected |
| 05-token-minting | the agent mints a valid OIDC token; claims are correct and the signing `kid` is published in the issuer JWKS |
| 08-no-mutation-negative | pods with no binding are left unmodified |
| 09-rejection-negative | a pod referencing an invalid binding is rejected |

## Why there is no live STS / S3 test

The flow that actually exchanges the minted token for AWS credentials
(`sts:AssumeRoleWithWebIdentity` → use the creds against S3) is **not** tested
here, because LocalStack community cannot emulate it for a custom OpenID Connect
provider:

- LocalStack requires the token `iss` to start with `https://`, but stores the
  registered provider URL scheme-stripped (`kif-issuer…:5002`) and then matches
  the full `https://…` `iss` against it, so the lookup always fails with
  `No OpenIDConnect provider found in your account for https://…`.
- This validation is LocalStack-specific — moto's `assume_role_with_web_identity`
  performs no web-identity validation at all — and has no documented toggle to
  relax it. Related: localstack/localstack#11838.
- The same minted token is accepted by **real** AWS STS.

Rather than ship a test that can never run, `05-token-minting` verifies the part
kif fully controls: the token's signing `kid` is present in the issuer's served
JWKS, so a relying party can fetch the key set and verify the signature. Truly
validating the STS exchange requires running against real AWS.
