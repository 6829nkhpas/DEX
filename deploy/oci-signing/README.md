# OCI Image Signing

Instructions for signing DEX Web UI container images using
[cosign](https://github.com/sigstore/cosign) (part of the Sigstore project).

## Prerequisites

```bash
# Install cosign
go install github.com/sigstore/cosign/v2/cmd/cosign@latest
# or
brew install cosign
```

## Generate a Key Pair (First Time)

```bash
cosign generate-key-pair

# This creates:
#   cosign.key  — private key (keep secret, store in Vault)
#   cosign.pub  — public key (distribute to verifiers)
```

## Sign an Image

After pushing an image to the registry:

```bash
# Sign with local key
cosign sign --key cosign.key ${REGISTRY}/dex-web-ui:${TAG}

# Sign with key from environment variable
COSIGN_KEY=$(cat cosign.key) cosign sign --key env://COSIGN_KEY ${REGISTRY}/dex-web-ui:${TAG}

# Keyless signing (uses OIDC identity, recommended for CI)
cosign sign ${REGISTRY}/dex-web-ui:${TAG}
```

## Verify a Signed Image

```bash
# Verify with public key
cosign verify --key cosign.pub ${REGISTRY}/dex-web-ui:${TAG}

# Keyless verification
cosign verify \
  --certificate-identity=ci@dex.example.com \
  --certificate-oidc-issuer=https://token.actions.githubusercontent.com \
  ${REGISTRY}/dex-web-ui:${TAG}
```

## CI Integration

Add to the `ci-build-and-deploy.yml` workflow after the push step:

```yaml
- name: Install cosign
  uses: sigstore/cosign-installer@v3

- name: Sign image
  env:
    COSIGN_KEY: ${{ secrets.COSIGN_KEY }}
  run: |
    cosign sign --key env://COSIGN_KEY \
      ${REGISTRY}/dex-web-ui:${GITHUB_SHA}
```

## Kubernetes Admission Policy

To enforce signed images in the cluster, use a policy engine:

```yaml
# Kyverno ClusterPolicy example
apiVersion: kyverno.io/v1
kind: ClusterPolicy
metadata:
  name: verify-dex-images
spec:
  validationFailureAction: Enforce
  rules:
    - name: verify-signature
      match:
        resources:
          kinds:
            - Pod
      verifyImages:
        - imageReferences:
            - "${REGISTRY}/dex-web-ui:*"
          attestors:
            - entries:
                - keys:
                    publicKeys: |-
                      -----BEGIN PUBLIC KEY-----
                      <your cosign.pub contents>
                      -----END PUBLIC KEY-----
```

## Secrets Required

| Secret            | Description                                 |
| ----------------- | ------------------------------------------- |
| `COSIGN_KEY`      | Private signing key (base64 or PEM)         |
| `COSIGN_PASSWORD` | Password for the private key (if encrypted) |
