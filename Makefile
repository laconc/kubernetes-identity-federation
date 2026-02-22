SHELL := /bin/bash

IMAGE_TAG ?= latest
BUILD_DATE := $(shell date -u +'%Y-%m-%dT%H:%M:%SZ')
GIT_REF := $(shell git rev-parse HEAD)
VERSION := $(shell cd app; cargo metadata --no-deps --format-version=1 | jq -r '.packages[0].version')

IMAGE_CRATES := kif-agent kif-federation kif-issuer kif-webhook

.PHONY: build-image crdgen lint test verify-crd

build-image:
	docker build \
		--build-arg BUILD_DATE=$(BUILD_DATE) \
		--build-arg GIT_REF=$(GIT_REF) \
		--build-arg VERSION=$(VERSION) \
		--build-arg BIN=$(BIN) \
		-t $(BIN):$(IMAGE_TAG) \
		.

build-images:
	@for crate in $(IMAGE_CRATES); do \
		echo "Building image for $$crate..."; \
		$(MAKE) build-image BIN=$$crate || exit 1; \
	done

crdgen:
	cd app && \
		RUST_BACKTRACE=1 cargo run -p kif-crdgen > ../deploy/crd/cloudrolebinding.yaml

verify-crd:
	@cd app && RUST_BACKTRACE=1 cargo run -p kif-crdgen \
		| diff -u ../deploy/crd/cloudrolebinding.yaml - \
		|| (echo "ERROR: The CloudRoleBinding CRD is out of date, please regenerate the CRD locally with 'make crdgen'."; exit 1)

lint:
	cd app && cargo clippy --all-targets --all-features -- -D warnings
	cd app && cargo fmt --all -- --check

test:
	cd app && cargo test --workspace --all-targets
