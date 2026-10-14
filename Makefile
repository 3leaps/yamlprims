PYTHON ?= python3

.PHONY: help check lint fmt version test-release-tag-controls release-tag release-verify-tag release-push-tag release-verify-remote-tag

help:
	@echo "make check   Validate specification-release files and local links"
	@echo "make version Show the specification revision"
	@echo "make test-release-tag-controls Test signing guards using disposable keys"
	@echo "make release-tag Create and verify a local signed specification tag"

check:
	$(PYTHON) scripts/check-docs.py

lint: check

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
