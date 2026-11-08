"""Naming provenance failures remain actionable without a Rust parser dependency."""

import contextlib
import hashlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from scripts.check_provenance import (
    LEDGER_NAME,
    check_provenance,
    check_rust,
    main,
    rust_functions,
)

HEADER = "// SPDX-License-Identifier: MIT\n// Origin: test implementation\n"
SOURCE = {"project": "upstream", "path": "upstream/source.py", "license": "MIT", "ref": "v1"}


class ProvenanceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # A miniature tree exercises the same recursive inventory as production.
        roots = patch("scripts.check_provenance.REQUIRED_ROOTS", ("naming",))
        fixtures = patch("scripts.check_provenance.REQUIRED_FIXTURES", ("fixtures/corpus.tsv",))
        roots.start()
        fixtures.start()
        self.addCleanup(roots.stop)
        self.addCleanup(fixtures.stop)
        self.entries = []
        self.add_file("naming/lib.rs", HEADER + "pub fn name() {}\n")
        self.add_file("fixtures/corpus.tsv", "smiles\tname\nC\tmethane\n")

    def add_file(self, path, contents, **metadata):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(contents.encode() if isinstance(contents, str) else contents)
        entry = {"path": path, "license": "MIT", "origin": "original", **metadata}
        if target.suffix != ".rs":
            entry["sha256"] = hashlib.sha256(target.read_bytes()).hexdigest()
        self.entries.append(entry)
        return target

    def write_ledger(self):
        lines = []
        for entry in self.entries:
            lines.append("[[file]]")
            for key, value in entry.items():
                lines.append(f"{key} = {json.dumps(value)}")
        (self.root / LEDGER_NAME).write_text("\n".join(lines), encoding="utf-8")

    def check(self):
        self.write_ledger()
        return check_provenance(self.root)

    def test_complete_ledger_passes_and_all_listed_paths_are_checked(self):
        self.add_file("elsewhere/data.bin", b"\x00\xff\x01")
        self.assertEqual(self.check(), [])
        (self.root / "elsewhere/data.bin").write_bytes(b"changed")
        self.assertTrue(any("elsewhere/data.bin: SHA-256 mismatch" in e for e in self.check()))

    def test_recursive_inventory_includes_new_source_data_scripts_and_notices(self):
        for path in ("naming/deep/new.rs", "naming/data.xml", "naming/script.py", "naming/NOTICE"):
            with self.subTest(path=path):
                target = self.root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("new file", encoding="utf-8")
                self.assertIn(f"{path}: missing ledger entry", self.check())
                target.unlink()

    def test_required_fixture_cannot_be_dropped_from_ledger(self):
        self.entries.pop()
        self.assertIn("fixtures/corpus.tsv: missing ledger entry", self.check())

    def test_missing_ledger_path_fails(self):
        (self.root / "fixtures/corpus.tsv").unlink()
        self.assertIn("fixtures/corpus.tsv: ledger path does not exist as a file", self.check())

    def test_non_rust_missing_hash_and_changed_bytes_fail(self):
        self.entries[-1].pop("sha256")
        self.assertTrue(any("SHA-256 mismatch" in e for e in self.check()))
        self.entries[-1]["sha256"] = "0" * 64
        self.assertTrue(any("SHA-256 mismatch" in e for e in self.check()))

    def test_rust_body_changes_are_not_hashed(self):
        (self.root / "naming/lib.rs").write_text(HEADER + "fn revised() {}", encoding="utf-8")
        self.assertEqual(self.check(), [])

    def test_missing_invalid_and_duplicate_ledger_entries_fail(self):
        self.assertIn("cannot read provenance ledger", check_provenance(self.root)[0])
        (self.root / LEDGER_NAME).write_text("not valid toml", encoding="utf-8")
        self.assertIn("cannot read provenance ledger", check_provenance(self.root)[0])
        (self.root / LEDGER_NAME).write_text("schema_version = 2", encoding="utf-8")
        self.assertIn("expected [[file]]", check_provenance(self.root)[0])
        self.entries.append(self.entries[0])
        self.assertIn("naming/lib.rs: duplicate ledger entry", self.check())
        self.entries[-1] = {"path": "../outside.rs"}
        self.assertTrue(any("invalid repository-relative path" in e for e in self.check()))

    def test_rust_headers_must_open_file_and_match_expression(self):
        entry = {"origin": "original", "license": "MIT OR Apache-2.0"}
        errors = check_rust("sample.rs", HEADER + "fn x() {}", entry)
        self.assertTrue(any("SPDX header must match" in error for error in errors))
        for text in (
            "fn x() {}\n" + HEADER,
            "// SPDX-License-Identifier: MIT\nfn x() {}\n",
            "// Origin: \n// SPDX-License-Identifier: MIT\nfn x() {}\n",
            "/*\n// SPDX-License-Identifier: MIT\n// Origin: fake\n*/\nfn x() {}\n",
        ):
            with self.subTest(text=text):
                self.assertTrue(
                    check_rust("sample.rs", text, {"origin": "original", "license": "MIT"})
                )

    def test_cli_exit_codes_and_diagnostics(self):
        self.write_ledger()
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(main(["--root", str(self.root)]), 0)
        self.assertIn("match the ledger", output.getvalue())
        (self.root / "fixtures/corpus.tsv").write_text("changed", encoding="utf-8")
        with contextlib.redirect_stderr(io.StringIO()) as output:
            self.assertEqual(main(["--root", str(self.root)]), 1)
        self.assertIn("fixtures/corpus.tsv: SHA-256 mismatch", output.getvalue())


class RustBoundaryTests(unittest.TestCase):
    def entry(self, items, origin="adapted"):
        return {
            "origin": "mixed",
            "license": "MIT",
            "parts": [{"items": items, "origin": origin, "sources": [SOURCE]}],
        }

    def test_each_origin_requires_its_correct_marker_and_recorded_source(self):
        cases = (
            ("port", "// Ported from upstream/source.py::_upstream"),
            ("adapted", "// Adapted from upstream/source.py (exact mapping not recorded)"),
            ("adapted", "// Adapted from upstream/source.py, upstream/other.py (module mapping)"),
            ("mixed", "// Adapted from upstream/source.py::x (local Rust boundary)"),
            ("original", "// Original nazuchem code (not in upstream v1): fn x"),
        )
        for origin, marker in cases:
            with self.subTest(origin=origin):
                entry = self.entry(["fn x"], origin)
                self.assertEqual(check_rust("test.rs", HEADER + marker + "\nfn x() {}", entry), [])
                self.assertTrue(check_rust("test.rs", HEADER + "fn x() {}", entry))
        self.assertTrue(
            check_rust(
                "x.rs", HEADER + "// Ported from wrong.py\nfn x() {}", self.entry(["fn x"], "port")
            )
        )
        self.assertTrue(
            check_rust(
                "x.rs",
                HEADER + "// Adapted from upstream/source.py.extra\nfn x() {}",
                self.entry(["fn x"]),
            )
        )
        self.assertTrue(
            check_rust(
                "x.rs",
                HEADER + "// Ported from upstream/source.py\nfn x() {}",
                self.entry(["fn x"]),
            )
        )

    def test_marker_cannot_leak_from_previous_function(self):
        text = HEADER + "// Adapted from upstream/source.py\nfn x() {}\nfn y() {}"
        errors = check_rust("x.rs", text, self.entry(["fn x", "fn y"]))
        self.assertEqual(len(errors), 1)
        self.assertIn("fn y missing", errors[0])

    def test_docs_attributes_and_literals_do_not_hide_real_boundaries(self):
        text = (
            HEADER
            + """
// Adapted from upstream/source.py
/// Example documentation.
#[cfg(any(test, feature = "fn fake() { }"))]
#[custom({ fn ignored() {} })]
pub(crate) async fn real<'a>(value: &'a str) {
    let _ = r###" fn fake() { } /* comment */ "###;
    let _ = "fn fake() {}";
    let _ = '{';
    /* nested /* fn fake() {} */ comment */
}
"""
        )
        functions = rust_functions(text)
        self.assertEqual([function.name for function in functions], ["real"])
        self.assertEqual(check_rust("x.rs", text, self.entry(["fn real"])), [])

    def test_marker_text_inside_block_comment_or_literal_does_not_count(self):
        for prelude in (
            "/*\n// Adapted from upstream/source.py\n*/",
            '#[doc = r"\n// Adapted from upstream/source.py\n"]',
        ):
            self.assertTrue(
                check_rust("x.rs", HEADER + prelude + "\nfn x() {}", self.entry(["fn x"]))
            )

    def test_qualified_methods_nested_functions_and_all_duplicate_names(self):
        text = (
            HEADER
            + """
impl<'a, T> Trait<T> for First<'a, T> {
    // Adapted from upstream/source.py
    fn new() {}
}
impl Second {
    fn new() {}
}
fn outer() {
    // Adapted from upstream/source.py
    fn recur() {}
}
#[cfg(test)]
mod tests {
    fn recur() {}
}
"""
        )
        names = [function.qualified_name for function in rust_functions(text)]
        self.assertEqual(
            names, ["First::new", "Second::new", "outer", "outer::recur", "tests::recur"]
        )
        self.assertEqual(
            check_rust("x.rs", text, self.entry(["fn First::new", "fn outer::recur"])), []
        )
        errors = check_rust("x.rs", text, self.entry(["fn new", "fn recur"]))
        self.assertEqual(len(errors), 2)
        self.assertTrue(any("fn new missing" in error for error in errors))
        self.assertTrue(any("fn recur missing" in error for error in errors))

    def test_missing_function_is_actionable_and_nonmixed_files_need_no_marker(self):
        errors = check_rust("x.rs", HEADER, self.entry(["fn missing"]))
        self.assertIn("has no Rust function", errors[0])
        self.assertEqual(
            check_rust("x.rs", HEADER + "fn x() {}", {"origin": "port", "license": "MIT"}), []
        )


if __name__ == "__main__":
    unittest.main()
