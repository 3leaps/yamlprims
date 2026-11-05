.DEFAULT_GOAL := help
SHELL := bash
.SHELLFLAGS := -eu -o pipefail -c

CARGO ?= cargo
GO ?= go
GOENV := GOWORK=off CGO_ENABLED=0
GOENV_RACE := GOWORK=off CGO_ENABLED=1
PYTHON ?= python3
YAMLFMT ?= yamlfmt
YAMLLINT ?= yamllint
SHELLCHECK ?= shellcheck
ACTIONLINT ?= actionlint
GONEAT ?= goneat
MSRV := $(shell sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml)
VERSION := $(shell tr -d ' \t\r\n' < VERSION)
SHELL_SOURCES := $(wildcard scripts/*.sh tests/release/*.sh conformance/*.sh)

.PHONY: all help prepush precommit pr-final check-tooling lint-workflows
.PHONY: check test-docs lint lint-shell lint-yaml fmt-yaml fmt fmt-check
.PHONY: test-rust check-rust msrv-check check-go test-go test-go-race test-conformance-tools build clean
.PHONY: version spec-version version-check check-packages deps-check deny
.PHONY: workflow-pin jq-portability content-check host-path-check surface-scan doctor
.PHONY: test-release-tag-controls release-tooling-test release-check release-github-preflight
.PHONY: release-preflight release-prepare-tag-message release-tag release-push-tag
.PHONY: release-verify-tag release-verify-remote-tag release-record-anchor release-export-pin
.PHONY: release-validate-pin release-insert-anchors release-crates-list release-source-check
.PHONY: release-crates-fetch release-crate-dry-run release-crate-publish-dry-run release-crate-publish
.PHONY: release-crates-clean release-clean release-download release-fetch-ci-artifacts
.PHONY: release-notes release-stage-public release-checksums release-create-draft release-stage
.PHONY: release-sign release-verify release-upload release-publish release
.PHONY: release-stage-published release-verify-published

all: check

help:
	@echo "make check-rust test-rust Check and test the Rust baseline"
	@echo "make test-conformance-tools Test qualification failure propagation"
	@echo "make prepush Run full local gates (Unix, installed tools required)"
	@echo "make check-tooling Verify the pinned goneat version"
	@echo "make lint-workflows Run actionlint on workflows"
	@echo "make check   Validate specification-release files and local links"
	@echo "make test-docs Test documentation validation using temporary fixtures"
	@echo "make lint-yaml Check YAML formatting and lint (two-space indentation)"
	@echo "make fmt-yaml Format YAML configuration"
	@echo "make lint-shell Check release scripts with Bash and ShellCheck"
	@echo "make version Show the specification revision"
	@echo "make test-release-tag-controls Test signing guards using disposable keys"
	@echo "make release-tooling-test Run all synthetic release-tooling controls"
	@echo "make release-tag Create and verify a local signed release or specification tag"

test-rust:
	$(CARGO) test --locked --workspace -- --test-threads=1

check-rust:
	$(CARGO) fmt --all --check
	$(CARGO) check --locked --workspace --all-targets

msrv-check:
	$(CARGO) +$(MSRV) test --locked --workspace -- --test-threads=1

check-go:
	@out="$$(gofmt -l yaml)"; if [ -n "$$out" ]; then echo "gofmt: $$out"; exit 1; fi
	$(GOENV) $(GO) vet ./yaml/...
	$(GOENV) $(GO) build ./yaml/...

test-go:
	$(GOENV) $(GO) test -count=1 ./yaml/...

test-go-race:
	$(GOENV_RACE) $(GO) test -race -count=1 ./yaml/...

test-conformance-tools:
	$(PYTHON) -B -m unittest discover -s conformance -p 'test_*.py'

precommit: fmt-check lint
	@echo "[ok] precommit"

# Recipes keep prerequisite validation ahead of every gate, even with make -j.
prepush:
	$(PYTHON) scripts/check-tooling.py --prepush --goneat "$(GONEAT)" \
		--require "$(ACTIONLINT)" --require "$(YAMLFMT)" --require "$(YAMLLINT)" \
		--require "$(SHELLCHECK)" --require bash --require git --require gpg --require "$(GO)" --require "$(MAKE)"
	$(MAKE) lint test-docs lint-workflows
	$(MAKE) deps-check content-check jq-portability version-check workflow-pin
	$(MAKE) check-rust test-rust check-go test-go test-go-race test-conformance-tools
	$(MAKE) surface-scan

pr-final: prepush
	@YAMLPRIMS_SURFACE_SCAN_REQUIRED=1 PYTHON=$(PYTHON) scripts/surface-scan.sh
	@echo "[ok] pr-final"

surface-scan:
	@PYTHON=$(PYTHON) scripts/surface-scan.sh

content-check:
	@$(PYTHON) -I -B scripts/content-check.py

host-path-check:
	@scripts/host-path-check.test.sh

jq-portability:
	@PYTHON=$(PYTHON) scripts/jq-portability.sh

deps-check:
	scripts/deps-check.sh
	@echo "[ok] deps-check"

deny:
	$(CARGO) deny --locked check

workflow-pin:
	$(PYTHON) -B -E scripts/release-workflow-pin.py check

doctor:
	@scripts/doctor.sh $(DOCTOR_ARGS)

check-tooling:
	$(PYTHON) scripts/check-tooling.py --goneat "$(GONEAT)"

lint-workflows:
	$(ACTIONLINT)

check:
	$(PYTHON) scripts/check-docs.py

test-docs:
	$(PYTHON) -B -m unittest discover -s tests/docs

lint: check lint-shell lint-yaml

lint-yaml:
	$(YAMLFMT) -dstar -lint .yamlfmt .yamllint .fulmen/repo.yaml '.github/**/*.yml' '.github/**/*.yaml' '.goneat/**/*.yaml'
	$(YAMLLINT) --strict -c .yamllint .yamlfmt .yamllint .fulmen/repo.yaml .github .goneat

fmt-yaml:
	$(YAMLFMT) -dstar .yamlfmt .yamllint .fulmen/repo.yaml '.github/**/*.yml' '.github/**/*.yaml' '.goneat/**/*.yaml'

lint-shell:
	@set -e; for script in $(SHELL_SOURCES); do bash -n "$$script"; done
	$(SHELLCHECK) --external-sources $(SHELL_SOURCES)

fmt:
	$(CARGO) fmt --all
	gofmt -w yaml
	@echo "Markdown uses UTF-8, LF and no trailing whitespace; make check verifies these rules."

fmt-check:
	$(CARGO) fmt --all -- --check
	@out="$$(gofmt -l yaml)"; if [ -n "$$out" ]; then echo "gofmt: $$out"; exit 1; fi

version:
	@cat SPEC_VERSION

spec-version:
	@cat SPEC_VERSION

version-check:
	@test "$(VERSION)" = "$$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)" || { echo "VERSION and Cargo.toml differ"; exit 1; }
	@echo "[ok] version-check"

build:
	$(CARGO) build --release --locked -p yamlprims-cli

clean:
	$(CARGO) clean

# -----------------------------------------------------------------------------
# Release (crucible PDR-0008)
# -----------------------------------------------------------------------------

.NOTPARALLEL:

ifneq ($(origin RELEASE_TAG),undefined)
$(error RELEASE_TAG is not accepted; set YAMLPRIMS_RELEASE_TAG=vX.Y.Z or spec-vX.Y)
endif
REQUIRE_RELEASE_TAG = @if [ -z "$(YAMLPRIMS_RELEASE_TAG)" ]; then \
	echo "error: YAMLPRIMS_RELEASE_TAG=vX.Y.Z or spec-vX.Y is required"; exit 1; fi

DIST_RELEASE := dist/release
DIST_PUBLISHED := dist/published
DIST_CRATES := dist/crates
NOTES_NAME := v$(strip $(VERSION))

test-release-tag-controls:
	bash tests/release/signed-tag-test.sh

release-tooling-test:
	scripts/release-tooling-test.sh

check-packages: version-check
	scripts/check-packages.sh

release-check: check-packages

release-github-preflight:
	scripts/release-github-preflight.sh

release-preflight:
	@source scripts/release-decernor.sh && resolve_release_decernor && echo "[ok] checker $$RELEASE_DECERNOR_BIN"
	@test -z "$$(git status --porcelain)" || { echo "error: working tree not clean"; git status --short; exit 1; }
	@for f in "$${CARGO_HOME:-$$HOME/.cargo}/credentials.toml" "$${CARGO_HOME:-$$HOME/.cargo}/credentials"; do \
		test ! -e "$$f" || { echo "error: $$f exists; the release token lives only in YAMLPRIMS_CARGO_TOKEN_FILE"; exit 1; }; \
	done
	@test -z "$${CARGO_REGISTRY_TOKEN+x}$${CARGO_REGISTRIES_CRATES_IO_TOKEN+x}" || { echo "error: unset CARGO_REGISTRY_TOKEN"; exit 1; }
	@test "$$(git symbolic-ref --quiet --short HEAD)" = main || { echo "error: release from main"; exit 1; }
	git fetch --quiet origin main
	@test "$$(git rev-parse HEAD)" = "$$(git rev-parse origin/main)" || { echo "error: HEAD differs from origin/main"; exit 1; }
	scripts/release-github-preflight.sh
	$(PYTHON) -I scripts/release-source-check.py HEAD
	$(MAKE) prepush release-tooling-test release-check
	YAMLPRIMS_SURFACE_SCAN_REQUIRED=1 YAMLPRIMS_SURFACE_SCOPE=release PYTHON=$(PYTHON) scripts/surface-scan.sh
	@test -f "docs/releases/$(NOTES_NAME).md" || { echo "error: docs/releases/$(NOTES_NAME).md is required"; exit 1; }
	@grep -q '^## \[$(VERSION)\]' CHANGELOG.md || { echo "error: CHANGELOG.md has no [$(VERSION)] section"; exit 1; }
	@grep -q '^\[$(VERSION)\]: ' CHANGELOG.md || { echo "error: CHANGELOG.md has no [$(VERSION)] compare link"; exit 1; }
	@echo "[ok] release preflight; next: make release-prepare-tag-message"

release-prepare-tag-message:
	$(REQUIRE_RELEASE_TAG)
	scripts/release-tag-operator.sh prepare-message

release-tag:
	$(REQUIRE_RELEASE_TAG)
	scripts/release-tag-operator.sh local-tag

release-push-tag:
	$(REQUIRE_RELEASE_TAG)
	scripts/release-tag-operator.sh remote-push

release-verify-tag:
	$(REQUIRE_RELEASE_TAG)
	scripts/release-verify-tag.sh

release-verify-remote-tag:
	$(REQUIRE_RELEASE_TAG)
	scripts/release-verify-published-tag.sh

release-record-anchor:
	$(REQUIRE_RELEASE_TAG)
	scripts/release-record-anchor.sh

release-export-pin:
	scripts/release-export-pin.sh

release-validate-pin:
	scripts/release-validate-pin.sh

release-insert-anchors:
	scripts/release-insert-anchors.sh

release-crates-list:
	@$(PYTHON) scripts/release-crates.py list

release-source-check:
	$(REQUIRE_RELEASE_TAG)
	@commit="$$(git rev-parse --verify --quiet "refs/tags/$(YAMLPRIMS_RELEASE_TAG)^{commit}")" || { echo "error: $(YAMLPRIMS_RELEASE_TAG) is absent"; exit 1; }; \
	$(PYTHON) -I scripts/release-source-check.py "$$commit"

release-crates-fetch:
	$(REQUIRE_RELEASE_TAG)
	@test -n "$(CRATE)" || { echo "error: CRATE=<name> is required"; exit 1; }
	scripts/release-crates-fetch.sh "$(YAMLPRIMS_RELEASE_TAG)" "$(CRATE)" "$(DIST_CRATES)"

release-crate-dry-run:
	$(REQUIRE_RELEASE_TAG)
	@test -n "$(CRATE)" || { echo "error: CRATE=<name> is required"; exit 1; }
	YAMLPRIMS_CRATE_RECORDS="$(abspath $(DIST_CRATES))" scripts/release-crate-publish.sh "$(CRATE)"

release-crate-publish-dry-run:
	$(REQUIRE_RELEASE_TAG)
	YAMLPRIMS_CRATE_RECORDS="$(abspath $(DIST_CRATES))" scripts/release-crate-publish.sh "$(or $(CRATE),yamlprims)"

release-crate-publish:
	$(REQUIRE_RELEASE_TAG)
	@test -n "$(CRATE)" || { echo "error: CRATE=<name> is required"; exit 1; }
	YAMLPRIMS_CRATE_RECORDS="$(abspath $(DIST_CRATES))" scripts/release-crate-publish.sh --publish "$(CRATE)"

release-crates-clean:
	rm -rf "$(DIST_CRATES)"

release-clean: release-crates-clean
	rm -rf "$(DIST_RELEASE)" "$(DIST_RELEASE).anchor" "$(DIST_PUBLISHED)" "$(DIST_PUBLISHED).anchor"

release-download:
	$(REQUIRE_RELEASE_TAG)
	scripts/release-fetch-ci-artifacts.sh "$(YAMLPRIMS_RELEASE_TAG)" "$(DIST_RELEASE)"

release-fetch-ci-artifacts: release-download

release-notes:
	$(REQUIRE_RELEASE_TAG)
	scripts/release-stage-public.sh "$(DIST_RELEASE)"

release-stage-public: release-notes

release-checksums:
	$(REQUIRE_RELEASE_TAG)
	scripts/generate-checksums.sh "$(DIST_RELEASE)" "$(YAMLPRIMS_RELEASE_TAG)"

release-create-draft:
	$(REQUIRE_RELEASE_TAG)
	scripts/release-create-draft.sh "$(YAMLPRIMS_RELEASE_TAG)" "$(DIST_RELEASE)"

release-stage:
	$(REQUIRE_RELEASE_TAG)
	$(MAKE) release-clean
	$(MAKE) release-download
	$(MAKE) release-notes
	$(MAKE) release-checksums
	$(MAKE) release-create-draft
	@echo "[ok] unsigned draft staged in $(DIST_RELEASE); inspect SHA256SUMS, then run: make release-sign release-upload release-publish"

release-sign:
	$(REQUIRE_RELEASE_TAG)
	scripts/sign-release-assets.sh "$(YAMLPRIMS_RELEASE_TAG)" "$(DIST_RELEASE)"

release-verify:
	$(REQUIRE_RELEASE_TAG)
	scripts/verify-checksums.sh "$(DIST_RELEASE)"
	scripts/verify-staged-public.sh "$(DIST_RELEASE)"
	scripts/verify-signatures.sh "$(DIST_RELEASE)"

release-upload: release-verify
	scripts/upload-release-assets.sh "$(YAMLPRIMS_RELEASE_TAG)" "$(DIST_RELEASE)"

release-publish:
	$(REQUIRE_RELEASE_TAG)
	scripts/release-publish.sh "$(DIST_RELEASE)"

release:
	$(REQUIRE_RELEASE_TAG)
	$(MAKE) release-stage
	$(MAKE) release-sign
	$(MAKE) release-upload
	@echo "[ok] signed draft staged; make release-publish promotes it"

release-stage-published:
	$(REQUIRE_RELEASE_TAG)
	rm -rf "$(DIST_PUBLISHED)" "$(DIST_PUBLISHED).anchor"
	scripts/release-stage-published.sh "$(YAMLPRIMS_RELEASE_TAG)" "$(DIST_PUBLISHED)"

release-verify-published: release-stage-published
