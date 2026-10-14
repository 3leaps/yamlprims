#!/usr/bin/env python3
"""Check the documentation release without installing a parser or dependencies."""

from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
REQUIRED = (
    "README.md",
    "AGENTS.md",
    "CONTRIBUTING.md",
    "MAINTAINERS.md",
    "SECURITY.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "SPEC_VERSION",
    "RELEASE_NOTES.md",
    "docs/spec/LICENSE",
    "docs/spec/yamlprims-spec.md",
    "docs/build-spec.md",
    "docs/design-review.md",
    "docs/decisions/README.md",
)


def main():
    errors = []
    for name in REQUIRED:
        if not (ROOT / name).is_file():
            errors.append(f"missing required file: {name}")
    documents = sorted(ROOT.rglob("*.md"))
    for path in documents:
        if ".git" in path.relative_to(ROOT).parts:
            continue
        raw = path.read_bytes()
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            errors.append(f"{path.relative_to(ROOT)}: not UTF-8")
            continue
        if b"\r" in raw or not raw.endswith(b"\n"):
            errors.append(f"{path.relative_to(ROOT)}: use LF and a final newline")
        if any(line.rstrip() != line for line in text.splitlines()):
            errors.append(f"{path.relative_to(ROOT)}: trailing whitespace")
        # Code examples are illustrative, not Markdown links to source files.
        prose = re.sub(r"^```[^\n]*\n.*?^```[^\n]*$", "", text, flags=re.M | re.S)
        for match in re.finditer(r"\[[^\]\n]+\]\(([^\s)]+)\)", prose):
            link = urlsplit(match.group(1))
            if link.scheme or link.netloc or not link.path:
                continue
            target = (path.parent / unquote(link.path)).resolve()
            if not target.is_relative_to(ROOT) or not target.exists():
                errors.append(
                    f"{path.relative_to(ROOT)}: missing/escaping local link {match.group(1)}"
                )
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(
        f"Documentation checks passed ({len(documents)} Markdown files; local file targets only)."
    )
    print("No runtime, external URL, anchor, or YAML conformance checks claimed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
