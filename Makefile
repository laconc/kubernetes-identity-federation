SHELL := /bin/bash

IMAGE_TAG ?= latest
BUILD_DATE := $(shell date -u +'%Y-%m-%dT%H:%M:%SZ')
GIT_REF := $(shell git rev-parse HEAD)
VERSION := $(shell cd app; cargo metadata --no-deps --format-version=1 | jq -r '.packages[0].version')

IMAGE_CRATES := kif-agent kif-federation kif-issuer kif-webhook

ifdef GHA_CACHE
BINARY_CACHE_ARGS = \
	--cache-from type=gha,scope=deps \
	--cache-from type=gha,scope=$(BIN) \
	--cache-to type=gha,mode=max,scope=$(BIN)
DEPS_CACHE_ARGS = \
	--cache-from type=gha,scope=deps \
	--cache-to type=gha,mode=max,scope=deps
else
BINARY_CACHE_ARGS =
DEPS_CACHE_ARGS =
endif

.PHONY: build-image build-images $(IMAGE_CRATES:%=build-image-%) push-images $(IMAGE_CRATES:%=push-image-%) deps-cache crdgen lint test verify-crds

build-image:
	docker buildx build \
		--build-arg BUILD_DATE=$(BUILD_DATE) \
		--build-arg GIT_REF=$(GIT_REF) \
		--build-arg VERSION=$(VERSION) \
		--build-arg BIN=$(BIN) \
		$(BINARY_CACHE_ARGS) \
		--load \
		-t $(BIN):$(IMAGE_TAG) \
		.

build-images:
	$(MAKE) -j$(words $(IMAGE_CRATES)) $(IMAGE_CRATES:%=build-image-%)

build-image-%:
	@echo "Building image for $*..."
	$(MAKE) build-image BIN=$*

push-images:
	$(MAKE) -j$(words $(IMAGE_CRATES)) $(IMAGE_CRATES:%=push-image-%)

push-image-%:
	docker tag $*:$(IMAGE_TAG) $(IMAGE_PREFIX)/$*:$(IMAGE_TAG)
	docker push $(IMAGE_PREFIX)/$*:$(IMAGE_TAG)

deps-cache:
	docker buildx build \
		--target deps \
		$(DEPS_CACHE_ARGS) \
		.

crdgen:
	cd app && \
		RUST_BACKTRACE=1 cargo run -p kif-crdgen -- crb > ../deploy/crd/crb.yaml && \
		RUST_BACKTRACE=1 cargo run -p kif-crdgen -- rcrb > ../deploy/crd/rcrb.yaml

verify-crds:
	@cd app && RUST_BACKTRACE=1 cargo run -p kif-crdgen -- crb \
		| diff -u ../deploy/crd/crb.yaml - \
		|| (echo "ERROR: The CloudRoleBinding CRD is out of date, please regenerate the CRD locally with 'make crdgen'."; exit 1)

	@cd app && RUST_BACKTRACE=1 cargo run -p kif-crdgen -- rcrb \
		| diff -u ../deploy/crd/rcrb.yaml - \
		|| (echo "ERROR: The ResolvedCloudRoleBinding CRD is out of date, please regenerate the CRD locally with 'make crdgen'."; exit 1)

lint:
	cd app && cargo clippy --all-targets --all-features -- -D warnings
	cd app && cargo fmt --all -- --check

test:
	cd app && cargo test --workspace --all-targets
