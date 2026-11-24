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
    "NOTICE",
    "SPEC_VERSION",
    "RELEASE_NOTES.md",
    "docs/spec/LICENSE",
    "docs/spec/yamlprims-spec.md",
    "docs/build-spec.md",
    "docs/design-review.md",
    "docs/decisions/README.md",
)


# Default load limits quoted in user guides as `name: value`, checked against
# the Rust `impl Default for Limits` and the Go `DefaultLimits`.
LIMIT_GUIDES = ("docs/migrating-from-libyaml.md",)
RUST_LIMITS = "crates/yamlprims/src/options.rs"
GO_LIMITS = "yaml/options.go"
UNITS = {"": 1, "KiB": 1 << 10, "MiB": 1 << 20, "GiB": 1 << 30}


def _code_value(expr):
    """Evaluate a default such as `250_000` or `8 << 20`."""
    match = re.fullmatch(r"([0-9_]+)(?:\s*<<\s*([0-9]+))?", expr.strip())
    if match is None:
        return None
    value = int(match.group(1).replace("_", ""))
    return value << int(match.group(2)) if match.group(2) else value


def _code_limits(text, pattern):
    """Return the name -> value map of the first block matching pattern."""
    block = re.search(pattern, text, re.S)
    if block is None:
        return None
    limits = {}
    for name, expr in re.findall(r"^\s*(\w+):\s*([^,\n]+),", block.group(1), re.M):
        limits[name] = _code_value(expr)
    return limits


def _snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def check_default_limits(root, texts):
    """Diagnostics for guide limit values that differ from the code defaults."""
    rust_path, go_path = root / RUST_LIMITS, root / GO_LIMITS
    if not rust_path.is_file():
        return []
    errors = []
    rust = _code_limits(
        rust_path.read_text(encoding="utf-8"),
        r"impl Default for Limits \{.*?Limits \{(.*?)\n\s*\}",
    )
    if not rust or None in rust.values():
        return [f"{RUST_LIMITS}: cannot read the default Limits"]
    if go_path.is_file():
        go = _code_limits(
            go_path.read_text(encoding="utf-8"),
            r"func DefaultLimits\(\) Limits \{\s*return Limits\{(.*?)\n\s*\}",
        )
        if not go or None in go.values():
            errors.append(f"{GO_LIMITS}: cannot read DefaultLimits")
        elif {_snake(k): v for k, v in go.items()} != rust:
            errors.append(f"{GO_LIMITS}: DefaultLimits differs from {RUST_LIMITS}")
    for name in LIMIT_GUIDES:
        text = texts.get(name)
        if text is None:
            continue
        for match in re.finditer(
            r"`(max_\w+): ([0-9][0-9,]*)(?: ([KMG]iB))?`", text
        ):
            limit, digits, unit = match.group(1), match.group(2), match.group(3) or ""
            value = int(digits.replace(",", "")) * UNITS[unit]
            if limit not in rust:
                errors.append(f"{name}: `{limit}` is not a load limit")
            elif value != rust[limit]:
                errors.append(
                    f"{name}: `{limit}` default is {rust[limit]}, not {match.group(0)}"
                )
    return errors


def check(root):
    """Return diagnostics and the number of Markdown documents checked."""
    root = root.resolve()
    errors = []
    for name in REQUIRED:
        if not (root / name).is_file():
            errors.append(f"missing required file: {name}")
    documents = sorted(
        path
        for path in root.rglob("*.md")
        if ".git" not in path.relative_to(root).parts
        and path.relative_to(root).parts[0] not in ("target", "dist")
    )
    texts = {}
    for path in documents:
        raw = path.read_bytes()
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            errors.append(f"{path.relative_to(root)}: not UTF-8")
            continue
        texts[path.relative_to(root).as_posix()] = text
        if b"\r" in raw or not raw.endswith(b"\n"):
            errors.append(f"{path.relative_to(root)}: use LF and a final newline")
        if any(line.rstrip() != line for line in text.splitlines()):
            errors.append(f"{path.relative_to(root)}: trailing whitespace")
        # Code examples are illustrative, not Markdown links to source files.
        prose = re.sub(r"^```[^\n]*\n.*?^```[^\n]*$", "", text, flags=re.M | re.S)
        for match in re.finditer(r"\[[^\]\n]+\]\(([^\s)]+)\)", prose):
            link = urlsplit(match.group(1))
            if link.scheme or link.netloc or not link.path:
                continue
            target = (path.parent / unquote(link.path)).resolve()
            if not target.is_relative_to(root) or not target.exists():
                errors.append(
                    f"{path.relative_to(root)}: missing/escaping local link {match.group(1)}"
                )
    version_path = root / "SPEC_VERSION"
    if version_path.is_file():
        raw = version_path.read_bytes()
        if not re.fullmatch(rb"[0-9]+\.[0-9]+\n", raw):
            errors.append(
                "SPEC_VERSION: expected MAJOR.MINOR with LF and a final newline"
            )
        else:
            version = raw.decode("ascii").strip()
            spec = texts.get("docs/spec/yamlprims-spec.md")
            if spec is not None:
                revisions = re.findall(
                    r"^\*\*Revision ([0-9]+\.[0-9]+)\*\*", spec, re.M
                )
                if revisions != [version]:
                    errors.append(
                        "docs/spec/yamlprims-spec.md: revision must match SPEC_VERSION"
                    )
            notes = texts.get("RELEASE_NOTES.md")
            if notes is not None:
                first_line = notes.lstrip().splitlines()[0] if notes.strip() else ""
                heading = re.match(r"^# spec-v([0-9]+\.[0-9]+)(?:\s|$)", first_line)
                if heading is None or heading.group(1) != version:
                    errors.append(
                        "RELEASE_NOTES.md: first heading must match spec-v<SPEC_VERSION>"
                    )
    errors.extend(check_default_limits(root, texts))
    return errors, len(documents)


def main(root=ROOT):
    errors, count = check(root)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(
        f"Documentation checks passed ({count} Markdown files; local file targets only)."
    )
    print("No runtime, external URL, anchor, or YAML conformance checks claimed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
