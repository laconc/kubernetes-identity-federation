# syntax=docker/dockerfile:1
FROM public.ecr.aws/docker/library/rust:1.93-alpine AS builder

ARG BIN

WORKDIR /usr/src/app

RUN addgroup -S appuser -g 2000 && \
    adduser -S -D -H -G appuser -u 2000 appuser

RUN apk update && apk add build-base

COPY ./app ./
#
#RUN mkdir src && \
#    echo "fn main() {}" > src/main.rs && \
#    cargo build --release && \
#    rm -r src

RUN cargo build --release -p ${BIN}

# ----------------
FROM gcr.io/distroless/base:latest AS runtime

ARG BIN
ARG BUILD_DATE
ARG GIT_REF
ARG VERSION

# OCI labels for provenance
LABEL org.opencontainers.image.title="${BIN}" \
      org.opencontainers.image.description="" \
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

ENV RUST_BACKTRACE=1

CMD [ "./app" ]
