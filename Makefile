SHELL := /bin/bash

IMAGE_TAG ?= latest
IMAGE_PREFIX ?=
BUILD_DATE := $(shell date -u +'%Y-%m-%dT%H:%M:%SZ')
GIT_REF  := $(shell git rev-parse HEAD)
GIT_REPO ?= $(shell git remote get-url origin)
VERSION := $(shell cd app; cargo metadata --no-deps --format-version=1 | jq -r '.packages[0].version')

IMAGE_CRATES := kif-agent kif-federation kif-issuer kif-webhook

LOCAL_CACHE_DIR ?= .cache

ifdef GHA_CACHE
BINARY_CACHE_ARGS = \
	--cache-from type=gha,scope=deps \
	--cache-from type=gha,scope=$(BIN) \
	--cache-to type=gha,mode=max,scope=$(BIN)
DEPS_CACHE_ARGS = \
	--cache-from type=gha,scope=deps \
	--cache-to type=gha,mode=max,scope=deps
else
BINARY_CACHE_ARGS = \
	--cache-from type=local,src=$(LOCAL_CACHE_DIR)/deps \
	--cache-from type=local,src=$(LOCAL_CACHE_DIR)/$(BIN) \
	--cache-to type=local,dest=$(LOCAL_CACHE_DIR)/$(BIN),mode=max
DEPS_CACHE_ARGS = \
	--cache-from type=local,src=$(LOCAL_CACHE_DIR)/deps \
	--cache-to type=local,dest=$(LOCAL_CACHE_DIR)/deps,mode=max
endif

.PHONY: build-images push-images sign-images deps-cache crdgen lint test verify-crds e2e e2e-setup e2e-teardown

build-images: deps-cache
	$(MAKE) -j$(words $(IMAGE_CRATES)) $(IMAGE_CRATES:%=build-image-%)

build-image-%: BIN = $*
build-image-%:
	@mkdir -p $(LOCAL_CACHE_DIR)
	docker buildx build \
		--build-arg BUILD_DATE=$(BUILD_DATE) \
		--build-arg GIT_REF=$(GIT_REF) \
		--build-arg VERSION=$(VERSION) \
		--build-arg BIN=$* \
		$(BINARY_CACHE_ARGS) \
		--load \
		-t $*:$(IMAGE_TAG) \
		$(if $(IMAGE_PREFIX),-t $(IMAGE_PREFIX)/$*:$(IMAGE_TAG)) \
		.

push-images:
	$(MAKE) -j$(words $(IMAGE_CRATES)) $(IMAGE_CRATES:%=push-image-%)

push-image-%:
	@test -n "$(IMAGE_PREFIX)" || (echo "ERROR: IMAGE_PREFIX is required for push-images"; exit 1)
	docker image inspect $(IMAGE_PREFIX)/$*:$(IMAGE_TAG) >/dev/null 2>&1 || \
	  docker tag $*:$(IMAGE_TAG) $(IMAGE_PREFIX)/$*:$(IMAGE_TAG)
	docker push $(IMAGE_PREFIX)/$*:$(IMAGE_TAG)

sign-images:
	$(MAKE) -j$(words $(IMAGE_CRATES)) $(IMAGE_CRATES:%=sign-image-%)

sign-image-%:
	$(eval DIGEST := $(shell docker inspect --format='{{index .RepoDigests 0}}' $(IMAGE_PREFIX)/$*:$(IMAGE_TAG) | cut -d@ -f2))
	cosign sign \
		--yes \
		-a "repo=$(GIT_REPO)" \
		-a "sha=$(GIT_REF)" \
		$(if $(GITHUB_WORKFLOW),-a "workflow=$(GITHUB_WORKFLOW)") \
		$(IMAGE_PREFIX)/$*:$(IMAGE_TAG)@$(DIGEST)

deps-cache:
	@mkdir -p $(LOCAL_CACHE_DIR)
	docker buildx build \
		--target deps \
		$(DEPS_CACHE_ARGS) \
		.

crdgen:
	cd app && \
		RUST_BACKTRACE=1 cargo run -p kif-crdgen -- crb > ../deploy/charts/kif/templates/crds/crb.yaml && \
		RUST_BACKTRACE=1 cargo run -p kif-crdgen -- rcrb > ../deploy/charts/kif/templates/crds/rcrb.yaml

verify-crds:
	@cd app && RUST_BACKTRACE=1 cargo run -p kif-crdgen -- crb \
		| diff -u ../deploy/charts/kif/templates/crds/crb.yaml - \
		|| (echo "ERROR: CloudRoleBinding CRD is out of date, please run 'make crdgen'."; exit 1)

	@cd app && RUST_BACKTRACE=1 cargo run -p kif-crdgen -- rcrb \
		| diff -u ../deploy/charts/kif/templates/crds/rcrb.yaml - \
		|| (echo "ERROR: ResolvedCloudRoleBinding CRD is out of date, please run 'make crdgen'."; exit 1)

lint:
	cd app && cargo clippy --all-targets --all-features -- -D warnings
	cd app && cargo fmt --all -- --check

test:
	cd app && cargo test --workspace --all-targets

e2e: e2e-setup
	rc=0; ./e2e/run.sh || rc=$$?; $(MAKE) e2e-teardown; exit $$rc

e2e-setup:
	IMAGE_TAG=$(IMAGE_TAG) ./e2e/setup.sh

e2e-teardown:
	./e2e/teardown.sh
