# syntax=docker/dockerfile:1
FROM public.ecr.aws/docker/library/rust:1.96-alpine AS deps

WORKDIR /usr/src/app

RUN addgroup -S appuser -g 2000 && \
    adduser -S -D -H -G appuser -u 2000 appuser

RUN apk add --no-cache build-base

COPY ./app/Cargo.toml ./app/Cargo.lock ./
COPY ./app/crates/kif-api/Cargo.toml        crates/kif-api/Cargo.toml
COPY ./app/crates/kif-agent/Cargo.toml      crates/kif-agent/Cargo.toml
COPY ./app/crates/kif-crdgen/Cargo.toml     crates/kif-crdgen/Cargo.toml
COPY ./app/crates/kif-federation/Cargo.toml crates/kif-federation/Cargo.toml
COPY ./app/crates/kif-issuer/Cargo.toml     crates/kif-issuer/Cargo.toml
COPY ./app/crates/kif-webhook/Cargo.toml    crates/kif-webhook/Cargo.toml

RUN mkdir -p crates/kif-api/src        && touch                 crates/kif-api/src/lib.rs         && \
    mkdir -p crates/kif-agent/src      && echo 'fn main() {}' > crates/kif-agent/src/main.rs      && \
    mkdir -p crates/kif-crdgen/src     && echo 'fn main() {}' > crates/kif-crdgen/src/main.rs     && \
    mkdir -p crates/kif-federation/src && echo 'fn main() {}' > crates/kif-federation/src/main.rs && \
    mkdir -p crates/kif-issuer/src     && echo 'fn main() {}' > crates/kif-issuer/src/main.rs     && \
    mkdir -p crates/kif-webhook/src    && echo 'fn main() {}' > crates/kif-webhook/src/main.rs

# Fetch the dependencies and store them in their own layer
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --locked

# Remove stub artifacts for local crates so the builder stage can recompile them
# against real sources; external deps in target/release/deps are kept
RUN find target/release -maxdepth 1 -name "kif*" -delete && \
    find target/release/deps -name "kif*" -delete && \
    find target/release/.fingerprint -maxdepth 1 -name "kif*" -exec rm -rf {} +

# ----------------
FROM deps AS builder

ARG BIN

COPY ./app ./

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --locked -p ${BIN}

# ----------------
FROM gcr.io/distroless/base:latest AS runtime

ARG BIN
ARG BUILD_DATE
ARG GIT_REF
ARG VERSION

# OCI labels for provenance
LABEL org.opencontainers.image.title="${BIN}" \
      org.opencontainers.image.description="Allows Kubernetes workloads to auth and access AWS, Azure, and GCP resources using OIDC federation." \
      org.opencontainers.image.authors="Dashiel Lopez Mendez <hi@64f.dev>" \
      org.opencontainers.image.url="https://github.com/laconc/kubernetes-identity-federation" \
      org.opencontainers.image.source="https://github.com/laconc/kubernetes-identity-federation" \
      org.opencontainers.image.documentation="https://github.com/laconc/kubernetes-identity-federation/blob/${GIT_REF}/README.md" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.revision="${GIT_REF}" \
      org.opencontainers.image.base.name="gcr.io/distroless/base:latest" \
      org.opencontainers.image.ref.name="${VERSION}" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.licenses="Apache-2.0"

WORKDIR /usr/src/app

COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group
COPY --from=builder /usr/src/app/target/release/${BIN} ./app

USER appuser:appuser

CMD [ "./app" ]
