PYTHON ?= python3
YAMLFMT ?= yamlfmt
YAMLLINT ?= yamllint
SHELLCHECK ?= shellcheck
SHELL_SOURCES := $(wildcard scripts/*.sh tests/release/*.sh)

.PHONY: help check lint lint-shell lint-yaml fmt-yaml fmt version test-release-tag-controls release-tag release-verify-tag release-push-tag release-verify-remote-tag

help:
	@echo "make check   Validate specification-release files and local links"
	@echo "make lint-yaml Check YAML formatting and lint (two-space indentation)"
	@echo "make fmt-yaml Format YAML configuration"
	@echo "make lint-shell Check release scripts with Bash and ShellCheck"
	@echo "make version Show the specification revision"
	@echo "make test-release-tag-controls Test signing guards using disposable keys"
	@echo "make release-tag Create and verify a local signed specification tag"

check:
	$(PYTHON) scripts/check-docs.py

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
