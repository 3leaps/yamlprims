.DEFAULT_GOAL := help
PYTHON ?= python3
YAMLFMT ?= yamlfmt
YAMLLINT ?= yamllint
SHELLCHECK ?= shellcheck
ACTIONLINT ?= actionlint
GONEAT ?= goneat
SHELL_SOURCES := $(wildcard scripts/*.sh tests/release/*.sh conformance/*.sh)

.PHONY: test-rust check-rust test-conformance-tools

test-rust:
	cargo test --locked --workspace -- --test-threads=1

check-rust:
	cargo fmt --all --check
	cargo check --locked --workspace --all-targets

test-conformance-tools:
	$(PYTHON) -B -m unittest discover -s conformance -p 'test_*.py'

.PHONY: help prepush check-tooling lint-workflows check test-docs lint lint-shell lint-yaml fmt-yaml fmt version test-release-tag-controls release-tag release-verify-tag release-push-tag release-verify-remote-tag

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
	@echo "make release-tag Create and verify a local signed specification tag"

# Recipes keep prerequisite validation ahead of every gate, even with make -j.
prepush:
	$(PYTHON) scripts/check-tooling.py --prepush --goneat "$(GONEAT)" \
		--require "$(ACTIONLINT)" --require "$(YAMLFMT)" --require "$(YAMLLINT)" \
		--require "$(SHELLCHECK)" --require bash --require git --require gpg --require "$(MAKE)"
	$(MAKE) lint test-docs lint-workflows
	$(MAKE) test-release-tag-controls
	$(MAKE) check-rust test-rust test-conformance-tools

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
	$(YAMLFMT) -dstar -lint .yamlfmt .yamllint '.github/**/*.yml' '.github/**/*.yaml' '.goneat/**/*.yaml'
	$(YAMLLINT) --strict -c .yamllint .yamlfmt .yamllint .github .goneat

fmt-yaml:
	$(YAMLFMT) -dstar .yamlfmt .yamllint '.github/**/*.yml' '.github/**/*.yaml' '.goneat/**/*.yaml'

lint-shell:
	@set -e; for script in $(SHELL_SOURCES); do bash -n "$$script"; done
	$(SHELLCHECK) --external-sources $(SHELL_SOURCES)

fmt:
	@echo "Markdown uses UTF-8, LF and no trailing whitespace; make check verifies these rules."

version:
	@cat SPEC_VERSION

test-release-tag-controls:
	bash tests/release/signed-tag-test.sh

release-tag:
	bash scripts/release-tag.sh

release-verify-tag:
	bash scripts/release-verify-tag.sh

release-push-tag:
	bash scripts/release-push-tag.sh

release-verify-remote-tag:
	bash scripts/release-verify-remote-tag.sh
