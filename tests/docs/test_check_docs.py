"""Exercise documentation gates with synthetic files outside the checkout."""

import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[2] / "scripts" / "check-docs.py"
SPEC = importlib.util.spec_from_file_location("check_docs", SCRIPT)
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


class DocumentationChecks(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in checker.REQUIRED:
            self.write(name, "Synthetic document.\n")
        self.write("SPEC_VERSION", "1.3\n")
        self.write(
            "docs/spec/yamlprims-spec.md",
            "# Specification\n\n**Revision 1.3** · Status: Draft\n",
        )
        self.write(
            "RELEASE_NOTES.md", "# spec-v1.3 — draft\n\n## spec-v1.2 — historical\n"
        )

    def write(self, name, content):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(
            content.encode("utf-8") if isinstance(content, str) else content
        )

    def run_check(self):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            status = checker.main(self.root)
        return status, out.getvalue(), err.getvalue()

    def rejected(self, diagnostic):
        status, out, err = self.run_check()
        self.assertEqual(status, 1)
        self.assertNotIn("checks passed", out)
        self.assertIn(diagnostic, err)

    def test_valid_revision_and_historical_notes(self):
        status, out, err = self.run_check()
        self.assertEqual((status, err), (0, ""))
        self.assertIn("Documentation checks passed", out)

    def test_missing_required_file(self):
        (self.root / "SPEC_VERSION").unlink()
        self.rejected("missing required file: SPEC_VERSION")

    def test_cargo_output_excluded_but_nested_documentation_checked(self):
        self.write("target/package/generated/README.md", b"generated\xff\n")
        self.assertEqual(self.run_check()[0], 0)
        self.write("docs/target/README.md", b"authored\xff\n")
        self.rejected(f"{Path('docs/target/README.md')}: not UTF-8")

    def test_version_format(self):
        for version in (
            b"1.3",
            b"1.3\r\n",
            b"v1.3\n",
            b"1.3.0\n",
            b"1.3 \n",
            b"\xff\n",
        ):
            with self.subTest(version=version):
                self.write("SPEC_VERSION", version)
                self.rejected("SPEC_VERSION: expected MAJOR.MINOR")

    def test_spec_revision_mismatch_missing_or_duplicate(self):
        for text in (
            "**Revision 1.2**\n",
            "No revision.\n",
            "**Revision 1.3**\n**Revision 1.3**\n",
        ):
            with self.subTest(text=text):
                self.write("docs/spec/yamlprims-spec.md", text)
                self.rejected("revision must match SPEC_VERSION")

    def test_release_heading_must_be_current_not_historical(self):
        for text in ("# spec-v1.2\n\n## spec-v1.3\n", "# Releases\n", "\n"):
            with self.subTest(text=text):
                self.write("RELEASE_NOTES.md", text)
                self.rejected("first heading must match")

    def test_invalid_utf8(self):
        self.write("README.md", b"bad\xff\n")
        self.rejected("README.md: not UTF-8")

    def test_line_endings_and_missing_final_newline(self):
        for content in (b"text\r\n", b"text", b""):
            with self.subTest(content=content):
                self.write("README.md", content)
                self.rejected("README.md: use LF and a final newline")

    def test_trailing_whitespace(self):
        self.write("README.md", "text \n")
        self.rejected("README.md: trailing whitespace")

    def test_missing_local_link(self):
        self.write("README.md", "[missing](missing.md)\n")
        self.rejected("missing/escaping local link missing.md")

    def test_existing_link_outside_root(self):
        with tempfile.TemporaryDirectory(dir=self.root.parent) as sibling:
            target = Path(sibling) / "outside.md"
            target.write_bytes(b"Outside.\n")
            link = "../" + Path(sibling).name + "/outside.md"
            self.write("README.md", "[outside](" + link + ")\n")
            self.rejected("missing/escaping local link")

    def test_encoded_local_link_and_illustrative_fence(self):
        self.write("docs/with space.md", "Example.\n")
        self.write(
            "README.md",
            "[local](docs/with%20space.md#heading)\n\n```md\n[example](absent.md)\n```\n",
        )
        self.assertEqual(self.run_check()[0], 0)

    def test_external_and_anchor_links_not_claimed_as_validated(self):
        self.write(
            "README.md",
            "[remote](https://example.invalid/missing) [anchor](#missing)\n",
        )
        status, out, _ = self.run_check()
        self.assertEqual(status, 0)
        self.assertIn(
            "No runtime, external URL, anchor, or YAML conformance checks claimed.", out
        )


if __name__ == "__main__":
    unittest.main()
