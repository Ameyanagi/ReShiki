"""Naming provenance failures remain actionable without a Rust parser dependency."""

import contextlib
import hashlib
import io
import json
import subprocess
import tempfile
import tomllib
import unittest
from copy import deepcopy
from datetime import date
from pathlib import Path
from unittest.mock import patch

from scripts.check_provenance import (
    FROZEN_PENDING,
    LEDGER_NAME,
    check_provenance,
    check_rust,
    main,
    rust_functions,
    rust_items,
)

SOURCE = {
    "project": "upstream",
    "path": "upstream/source.py",
    "license": "MIT",
    "ref": "v1",
    "symbol": "upstream_name",
}
LOCAL = {
    "project": "nazuchem",
    "path": "src/local.rs",
    "license": "MIT",
    "ref": "local-v1",
    "symbol": "local_name",
}
MODULE_SOURCE = {**SOURCE, "symbol": "module scope (mapping uncertain)"}


def header(origin="original", sources=None):
    sources = [SOURCE] if sources is None else sources
    return f"// SPDX-License-Identifier: MIT\n// Origin: {origin}\n" + "".join(
        f"// Source: {source['project']} | {source['path']} | {source['ref']}\n"
        for source in sources
    )


def marker(identity, origin="adapted", sources=None):
    sources = [SOURCE] if sources is None else sources
    return f"// Provenance: {identity} | {origin}\n" + "".join(
        f"// Source: {source['project']} | {source['path']}::{source['symbol']}\n"
        for source in sources
    )


def toml_value(value):
    if isinstance(value, dict):
        return (
            "{ "
            + ", ".join(f"{json.dumps(key)} = {toml_value(item)}" for key, item in value.items())
            + " }"
        )
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    return json.dumps(value)


HEADER = header()
MIXED_HEADER = header("mixed")


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
        entry = {
            "path": path,
            "license": "MIT",
            "origin": "original",
            "sources": [SOURCE],
            **metadata,
        }
        if target.suffix != ".rs":
            entry["sha256"] = hashlib.sha256(target.read_bytes()).hexdigest()
        self.entries.append(entry)
        return target

    def write_ledger(self):
        lines = []
        for entry in self.entries:
            lines.append("[[file]]")
            for key, value in entry.items():
                lines.append(f"{key} = {toml_value(value)}")
        (self.root / LEDGER_NAME).write_text("\n".join(lines), encoding="utf-8", newline="\n")

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
                target.write_text("new file", encoding="utf-8", newline="\n")
                self.assertIn(f"{path}: missing ledger entry", self.check())
                target.unlink()

    def test_git_ignored_files_are_not_inventoried(self):
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        (self.root / ".gitignore").write_text(".DS_Store\n", encoding="utf-8", newline="\n")
        (self.root / "naming/.DS_Store").write_bytes(b"finder")
        self.assertEqual(self.check(), [])
        (self.root / "naming/untracked.rs").write_text("new", encoding="utf-8", newline="\n")
        self.assertIn("naming/untracked.rs: missing ledger entry", self.check())

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
        (self.root / "naming/lib.rs").write_text(
            HEADER + "fn revised() {}", encoding="utf-8", newline="\n"
        )
        self.assertEqual(self.check(), [])

    def test_region_member_tables_round_trip_through_the_ledger(self):
        self.add_file(
            "naming/mixed.rs",
            MIXED_HEADER
            + marker("mod tests")
            + "mod tests { fn check() {} }\n"
            + marker("impl Fixture")
            + "impl Fixture { fn build() {} }\n",
            origin="mixed",
            parts=[
                {
                    "items": ["mod tests", "impl Fixture"],
                    "origin": "adapted",
                    "sources": [SOURCE],
                    "members": {
                        "mod tests": ["fn tests::check"],
                        "impl Fixture": ["fn Fixture::build"],
                    },
                }
            ],
        )
        self.assertEqual(self.check(), [])

    def test_missing_invalid_and_duplicate_ledger_entries_fail(self):
        self.assertIn("cannot read provenance ledger", check_provenance(self.root)[0])
        (self.root / LEDGER_NAME).write_text("not valid toml", encoding="utf-8", newline="\n")
        self.assertIn("cannot read provenance ledger", check_provenance(self.root)[0])
        (self.root / LEDGER_NAME).write_text("schema_version = 2", encoding="utf-8", newline="\n")
        self.assertIn("expected [[file]]", check_provenance(self.root)[0])
        self.entries.append(self.entries[0])
        self.assertIn("naming/lib.rs: duplicate ledger entry", self.check())
        self.entries[-1] = {"path": "../outside.rs"}
        self.assertTrue(any("invalid repository-relative path" in e for e in self.check()))

    def test_rust_headers_must_open_file_and_match_expression(self):
        entry = {"origin": "original", "license": "MIT OR Apache-2.0", "sources": [SOURCE]}
        errors = check_rust("sample.rs", HEADER + "fn x() {}", entry)
        self.assertTrue(any("SPDX header must match" in error for error in errors))
        for text in (
            "fn x() {}\n" + HEADER,
            "// SPDX-License-Identifier: MIT\nfn x() {}\n",
            "// Origin: \n// SPDX-License-Identifier: MIT\nfn x() {}\n",
            "/*\n// SPDX-License-Identifier: MIT\n// Origin: fake\n*/\nfn x() {}\n",
        ):
            with self.subTest(text=text):
                self.assertTrue(check_rust("sample.rs", text, {**entry, "license": "MIT"}))

    def test_origin_classification_and_all_file_source_identities_are_exact(self):
        entry = {"origin": "original", "license": "MIT", "sources": [SOURCE, LOCAL]}
        valid = header(sources=[LOCAL, SOURCE]) + "fn x() {}"
        self.assertEqual(check_rust("sample.rs", valid, entry), [])
        for text in (
            valid.replace("// Origin: original", "// Origin: entirely original code."),
            valid.replace("// Origin: original", "// Origin: adapted"),
            valid.replace("// Origin: original", "// Origin: original\n// Origin: original"),
            valid.replace("// Source: upstream | upstream/source.py | v1\n", ""),
            valid.replace("upstream/source.py | v1", "upstream/source.py | WRONG_REF"),
            valid.replace("// Source: upstream", "// Source: OTHER_PROJECT"),
            valid.replace(
                "// Source: upstream", "// Source: upstream | extra\n// Source: upstream"
            ),
        ):
            with self.subTest(text=text):
                self.assertTrue(check_rust("sample.rs", text, entry))

    def test_unresolved_license_does_not_create_a_general_legacy_exemption(self):
        entry = {"origin": "original", "license": "NOASSERTION", "sources": [SOURCE]}
        text = (
            "// SPDX-License-Identifier: NOASSERTION\n// Origin: arbitrary legacy prose\nfn x() {}"
        )
        self.assertTrue(check_rust("another_pending.rs", text, entry))

    def test_only_the_two_exact_pending_source_and_ledger_snapshots_are_exempt(self):
        repository = Path(__file__).resolve().parents[1]
        ledger = tomllib.loads((repository / LEDGER_NAME).read_text(encoding="utf-8"))
        entries = {entry["path"]: entry for entry in ledger["file"]}
        self.assertEqual(len(FROZEN_PENDING), 2)
        for path in FROZEN_PENDING:
            with self.subTest(path=path):
                text = (repository / path).read_bytes().decode("utf-8")
                entry = entries[path]
                self.assertEqual(check_rust(path, text, entry), [])
                self.assertIn("source snapshot changed", check_rust(path, text + "\n", entry)[0])
                changed = deepcopy(entry)
                changed["notes"] += " altered"
                self.assertIn("ledger entry changed", check_rust(path, text, changed)[0])
                changed = deepcopy(entry)
                changed["sources"][0]["ref"] = "new reference"
                self.assertIn("ledger entry changed", check_rust(path, text, changed)[0])
                changed["sources"][0]["ref"] = date(2026, 11, 9)
                self.assertIn("ledger entry changed", check_rust(path, text, changed)[0])
                self.assertTrue(check_rust("elsewhere/" + Path(path).name, text, entry))

    def test_cli_exit_codes_and_diagnostics(self):
        self.write_ledger()
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(main(["--root", str(self.root)]), 0)
        self.assertIn("match the ledger", output.getvalue())
        (self.root / "fixtures/corpus.tsv").write_text("changed", encoding="utf-8", newline="\n")
        with contextlib.redirect_stderr(io.StringIO()) as output:
            self.assertEqual(main(["--root", str(self.root)]), 1)
        self.assertIn("fixtures/corpus.tsv: SHA-256 mismatch", output.getvalue())


class RustBoundaryTests(unittest.TestCase):
    def entry(self, items, origin="adapted", sources=None, members=None):
        sources = [SOURCE] if sources is None else sources
        part = {"items": items, "origin": origin, "sources": sources}
        if members is not None:
            part["members"] = members
        return {
            "origin": "mixed",
            "license": "MIT",
            "sources": sources,
            "parts": [part],
        }

    def test_each_origin_requires_its_correct_marker_and_recorded_source(self):
        for origin in ("port", "copy", "adapted", "mixed", "original"):
            with self.subTest(origin=origin):
                entry = self.entry(["fn x"], origin)
                text = MIXED_HEADER + marker("fn x", origin) + "fn x() {}"
                self.assertEqual(check_rust("test.rs", text, entry), [])
                self.assertTrue(check_rust("test.rs", MIXED_HEADER + "fn x() {}", entry))

    def test_local_identity_source_symbol_and_classification_must_match(self):
        entry = self.entry(["fn x"])
        valid = MIXED_HEADER + marker("fn x") + "fn x() {}"
        for text in (
            valid.replace("Provenance: fn x", "Provenance: fn WRONG_LOCAL"),
            valid.replace("::upstream_name", "::WRONG_SYMBOL"),
            valid.replace("| adapted", "| port"),
            valid.replace("upstream/source.py::", "upstream/source.py.extra::"),
            valid.replace("// Source: upstream | upstream/source.py::upstream_name\n", ""),
            MIXED_HEADER + "// Adapted from upstream/source.py::upstream_name\nfn x() {}",
        ):
            with self.subTest(text=text):
                self.assertTrue(check_rust("test.rs", text, entry))

    def test_all_sources_are_required_including_original_local_sources(self):
        sources = [SOURCE, LOCAL]
        entry = self.entry(["fn x"], sources=sources)
        opening = header("mixed", sources)
        self.assertEqual(
            check_rust("x.rs", opening + marker("fn x", sources=sources) + "fn x() {}", entry), []
        )
        for origin in ("adapted", "original"):
            with self.subTest(origin=origin):
                entry["parts"][0]["origin"] = origin
                text = opening + marker("fn x", origin, [SOURCE]) + "fn x() {}"
                self.assertTrue(check_rust("x.rs", text, entry))
        missing_symbol = deepcopy(entry)
        missing_symbol["parts"][0]["sources"][1].pop("symbol")
        self.assertTrue(check_rust("x.rs", opening + "fn x() {}", missing_symbol))

    def test_multiple_symbols_in_one_source_file_are_distinct_required_mappings(self):
        sources = [SOURCE, {**SOURCE, "symbol": "Class.field default expression"}]
        entry = self.entry(["fn x"], sources=sources)
        entry["sources"] = [SOURCE]
        text = MIXED_HEADER + marker("fn x", sources=sources) + "fn x() {}"
        self.assertEqual(check_rust("x.rs", text, entry), [])
        self.assertTrue(check_rust("x.rs", text.replace("::upstream_name", "::wrong"), entry))

    def test_same_source_symbol_at_multiple_pins_has_one_marker_and_all_header_refs(self):
        sources = [SOURCE, {**SOURCE, "ref": "v2"}]
        entry = self.entry(["fn x"], sources=sources)
        text = header("mixed", sources) + marker("fn x") + "fn x() {}"
        self.assertEqual(check_rust("x.rs", text, entry), [])
        missing_ref = text.replace("// Source: upstream | upstream/source.py | v2\n", "")
        self.assertTrue(check_rust("x.rs", missing_ref, entry))

    def test_duplicate_and_unexpected_marker_sources_are_rejected(self):
        entry = self.entry(["fn x"])
        text = MIXED_HEADER + marker("fn x")
        for extra in (
            "// Source: upstream | upstream/source.py::upstream_name\n",
            "// Source: nazuchem | src/local.rs::local_name\n",
            "// Provenance: fn x | adapted\n",
        ):
            self.assertTrue(check_rust("x.rs", text + extra + "fn x() {}", entry))

    def test_module_scope_sources_inherit_exact_validated_header_refs(self):
        sources = [SOURCE, MODULE_SOURCE]
        entry = self.entry(["fn x"], sources=sources)
        entry["sources"] = [SOURCE]
        text = MIXED_HEADER + marker("fn x") + "fn x() {}"
        self.assertEqual(check_rust("x.rs", text, entry), [])
        explicit = MIXED_HEADER + marker("fn x", sources=sources) + "fn x() {}"
        self.assertEqual(check_rust("x.rs", explicit, entry), [])
        # Inheritance never removes a function-specific symbol or tolerates extras.
        for invalid in (
            text.replace("// Source: upstream | upstream/source.py::upstream_name\n", ""),
            explicit.replace("::upstream_name", "::wrong_specific_symbol"),
            explicit.replace("::module scope (mapping uncertain)", "::wrong_module_symbol"),
            text.replace("fn x()", marker("fn x") + "fn x()"),
            explicit.replace(
                "fn x()", "// Source: upstream | upstream/source.py::upstream_name\nfn x()"
            ),
            explicit.replace(
                "fn x()",
                "// Source: upstream | upstream/source.py::module scope (mapping uncertain)\nfn x()",
            ),
        ):
            with self.subTest(text=invalid):
                self.assertTrue(check_rust("x.rs", invalid, entry))

    def test_only_module_scope_sources_need_no_function_source_lines(self):
        entry = self.entry(["fn x"], sources=[MODULE_SOURCE])
        text = MIXED_HEADER + marker("fn x", sources=[]) + "fn x() {}"
        self.assertEqual(check_rust("x.rs", text, entry), [])
        for invalid in (
            text.replace("upstream/source.py | v1", "upstream/source.py | WRONG_REF"),
            text.replace("upstream/source.py | v1", "another/source.py | v1"),
            text.replace("// Source: upstream | upstream/source.py | v1\n", ""),
            text.replace("// SPDX-License-Identifier: MIT\n", ""),
        ):
            with self.subTest(text=invalid):
                errors = check_rust("x.rs", invalid, entry)
                self.assertTrue(any("opening" in error for error in errors))
                self.assertTrue(any("provenance marker" in error for error in errors))

    def test_module_scope_inheritance_rejects_wrong_or_unrelated_ledger_source_refs(self):
        text = MIXED_HEADER + marker("fn x", sources=[]) + "fn x() {}"
        for change in ({"ref": "v2"}, {"path": "elsewhere.py"}, {"project": "other"}):
            with self.subTest(change=change):
                entry = self.entry(["fn x"], sources=[{**MODULE_SOURCE, **change}])
                entry["sources"] = [SOURCE]
                errors = check_rust("x.rs", text, entry)
                self.assertTrue(any("part sources must also occur" in error for error in errors))
                self.assertTrue(any("provenance marker" in error for error in errors))
        # Each pin behind a shared marker symbol must be in the validated header.
        entry = self.entry(["fn x"], sources=[MODULE_SOURCE, {**MODULE_SOURCE, "ref": "v2"}])
        both_refs = header("mixed", entry["sources"]) + marker("fn x", sources=[]) + "fn x() {}"
        self.assertEqual(check_rust("x.rs", both_refs, entry), [])
        self.assertTrue(check_rust("x.rs", text, entry))

    def test_marker_cannot_leak_from_previous_function(self):
        text = MIXED_HEADER + marker("fn x") + "fn x() {}\nfn y() {}"
        errors = check_rust("x.rs", text, self.entry(["fn x", "fn y"]))
        self.assertEqual(len(errors), 1)
        self.assertIn("fn y missing", errors[0])

    def test_docs_attributes_and_literals_do_not_hide_real_boundaries(self):
        text = (
            MIXED_HEADER
            + marker("fn real")
            + """
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
            "/*\n" + marker("fn x") + "*/",
            '#[doc = r"\n' + marker("fn x") + '"]',
        ):
            self.assertTrue(
                check_rust("x.rs", MIXED_HEADER + prelude + "\nfn x() {}", self.entry(["fn x"]))
            )

    def test_every_discovered_function_requires_a_two_way_exact_assignment(self):
        entry = self.entry(["fn x"])
        text = MIXED_HEADER + marker("fn x") + "fn x() {}"
        self.assertEqual(check_rust("x.rs", text, entry), [])
        errors = check_rust("x.rs", text + "\nfn uncredited_port() {}", entry)
        self.assertTrue(any("fn uncredited_port has no mixed-file" in e for e in errors))
        errors = check_rust("x.rs", text.replace("fn x()", "fn renamed()"), entry)
        self.assertTrue(any("has no Rust declaration" in e for e in errors))
        self.assertTrue(any("fn renamed has no mixed-file" in e for e in errors))

    def test_unicode_function_module_and_method_identifiers_are_covered(self):
        text = (
            MIXED_HEADER
            + "mod 試験 { impl 型 {\n"
            + marker("fn 試験::型::名前")
            + "fn r#名前() {}\n}\n"
            + marker("fn 試験::café")
            + "fn cafe\u0301() {}\n}"
        )
        self.assertEqual(
            [item.identity for item in rust_items(text)],
            ["mod 試験", "impl 試験::型", "fn 試験::型::名前", "fn 試験::café"],
        )
        entry = self.entry(["fn 試験::型::名前", "fn 試験::café"])
        self.assertEqual(check_rust("x.rs", text, entry), [])
        errors = check_rust("x.rs", text + "\nfn 未帰属() {}", entry)
        self.assertTrue(
            any("fn 未帰属 has no mixed-file provenance assignment" in e for e in errors)
        )

    def test_unicode_append_to_real_mixed_file_is_rejected(self):
        repository = Path(__file__).resolve().parents[1]
        path = "src/naming/openclatura/principal_suffixes.rs"
        ledger = tomllib.loads((repository / LEDGER_NAME).read_text(encoding="utf-8"))
        entry = next(item for item in ledger["file"] if item["path"] == path)
        text = (repository / path).read_text(encoding="utf-8")
        errors = check_rust(path, text + "\nfn 未帰属() {}\n", entry)
        self.assertTrue(
            any("fn 未帰属 has no mixed-file provenance assignment" in e for e in errors)
        )

    def test_upstream_test_injected_into_original_region_requires_an_inventory_decision(self):
        repository = Path(__file__).resolve().parents[1]
        path = "src/naming/openclatura/principal_suffixes.rs"
        ledger = tomllib.loads((repository / LEDGER_NAME).read_text(encoding="utf-8"))
        entry = next(item for item in ledger["file"] if item["path"] == path)
        text = (repository / path).read_text(encoding="utf-8")
        self.assertEqual(check_rust(path, text, entry), [])
        region = next(item for item in rust_items(text) if item.identity == "mod tests")
        region_part = next(part for part in entry["parts"] if "mod tests" in part["items"])
        self.assertEqual(region_part["origin"], "original")
        identity = "fn tests::upstream_template_positions"
        # Rust rendition of the expectations in upstream tests/unit/test_analysis.py
        # ::test_principal_suffixes_render_programmatically_from_template_positions.
        # It is checker input only; neither this Rust nor upstream Python is executed.
        injection = """
    #[test]
    fn upstream_template_positions() {
        for (key, count, expected) in [
            ("carboxylic_acid", 1, "oic acid"),
            ("carboxylic_acid", 2, "dioic acid"),
            ("carboxylic_acid", 3, "trioic acid"),
            ("acid_chloride", 2, "dioyl dichloride"),
            ("acid_chloride", 3, "trioyl trichloride"),
            ("hydrazone", 2, "dione dihydrazone"),
            ("hydrazone", 3, "trione trihydrazone"),
        ] {
            let rule = RULES.functional_groups.get(key).unwrap();
            assert_eq!(render_principal_suffix(rule, count), expected);
        }
    }
"""
        changed = text[: region.end - 1] + injection + text[region.end - 1 :]
        errors = check_rust(path, changed, entry)
        self.assertTrue(
            any("members inventory missing" in error and identity in error for error in errors)
        )
        upstream = next(source for source in entry["sources"] if source["project"] == "openclatura")
        test_source = {
            **upstream,
            "path": "tests/unit/test_analysis.py",
            "symbol": "test_principal_suffixes_render_programmatically_from_template_positions",
        }
        entry = deepcopy(entry)
        source_fields = ("project", "path", "ref")
        if not any(
            all(source[field] == test_source[field] for field in source_fields)
            for source in entry["sources"]
        ):
            entry["sources"].append(test_source)
            changed = changed.replace(
                "// Origin: mixed\n",
                "// Origin: mixed\n"
                + f"// Source: openclatura | {test_source['path']} | {test_source['ref']}\n",
                1,
            )
        entry["parts"].append({"items": [identity], "origin": "adapted", "sources": [test_source]})
        changed = changed.replace(
            "    #[test]\n    fn upstream_template_positions()",
            marker(identity, sources=[test_source])
            + "    #[test]\n    fn upstream_template_positions()",
            1,
        )
        # An explicit fn override still must appear in every enclosing region's inventory.
        errors = check_rust(path, changed, entry)
        self.assertTrue(
            any("members inventory missing" in error and identity in error for error in errors)
        )
        region_part = next(part for part in entry["parts"] if "mod tests" in part["items"])
        region_part["members"]["mod tests"].append(identity)
        self.assertEqual(check_rust(path, changed, entry), [])
        missing_specific_source = changed.replace(
            f"// Source: openclatura | {test_source['path']}::{test_source['symbol']}\n", ""
        )
        self.assertTrue(check_rust(path, missing_specific_source, entry))

    def test_unicode_identifier_boundaries_and_rust_directional_whitespace(self):
        text = (
            "use cafe\u0301fn as other; use r#fn as alias;\n"
            "fn\u200e未帰属() {}\nmod\u200f試験 { fn ℘a·b١() {} }"
        )
        self.assertEqual(
            [item.identity for item in rust_items(text)],
            ["fn 未帰属", "mod 試験", "fn 試験::℘a·b١"],
        )

    def test_unsupported_unicode_identifier_fails_closed(self):
        text = MIXED_HEADER + marker("fn x") + "fn x() {}\nfn a\u200db() {}"
        errors = check_rust("x.rs", text, self.entry(["fn x"]))
        self.assertTrue(any("cannot verify complete Rust inventory" in e for e in errors))
        self.assertTrue(any("unsupported Rust identifier" in e for e in errors))

    def test_nested_declarations_in_const_generic_signatures_fail_closed(self):
        for declaration in (
            "fn outer<T: Trait<{ fn hidden() {} 0 }>>() {}",
            "fn outer() where (): Trait<{ fn hidden() {} 0 }> {}",
            "fn outer<T: Trait<{ mod hidden { fn inner() {} } 0 }>>() {}",
        ):
            with self.subTest(declaration=declaration):
                text = MIXED_HEADER + marker("fn outer") + declaration
                errors = check_rust("x.rs", text, self.entry(["fn outer"]))
                self.assertTrue(any("cannot verify complete Rust inventory" in e for e in errors))
                self.assertTrue(any("unsupported signature expression" in e for e in errors))

    def test_signature_expression_blocks_without_declarations_remain_supported(self):
        for declaration in (
            "fn outer<T: Trait<{ 0 }>>() {}",
            "fn outer(Point { x, y }: Point) {}",
        ):
            with self.subTest(declaration=declaration):
                text = MIXED_HEADER + marker("fn outer") + declaration
                self.assertEqual(check_rust("x.rs", text, self.entry(["fn outer"])), [])

    def test_inherent_impl_type_paths_have_distinct_qualified_identities(self):
        text = "impl a::Foo { fn f() {} } impl b::Foo { fn f() {} }\nimpl 模組::型 { fn 方法() {} }"
        self.assertEqual(
            [item.identity for item in rust_items(text)],
            [
                "impl a::Foo",
                "fn a::Foo::f",
                "impl b::Foo",
                "fn b::Foo::f",
                "impl 模組::型",
                "fn 模組::型::方法",
            ],
        )

    def test_colliding_concrete_generic_impl_identities_fail_closed(self):
        text = (
            MIXED_HEADER
            + "impl Foo<u8> {\n"
            + marker("fn Foo::f")
            + "fn f() {} }\n"
            + "impl Foo<u16> {\n"
            + marker("fn Foo::f")
            + "fn f() {} }"
        )
        errors = check_rust("x.rs", text, self.entry(["fn Foo::f"]))
        self.assertTrue(any("ambiguous impl identity 'Foo'" in e for e in errors))

    def test_qualified_methods_nested_functions_and_duplicate_leaf_names(self):
        text = (
            MIXED_HEADER
            + "impl<'a, T> Trait<T> for First<'a, T> {\n"
            + marker("fn <First<'a,T> as Trait<T>>::new")
            + "fn new() {}\n}\n"
            + "impl Second {\n"
            + marker("fn Second::new")
            + "fn new() {}\n}\n"
            + marker("fn outer")
            + "fn outer() {\n"
            + marker("fn outer::recur")
            + "fn recur() {}\n}\n"
            + "#[cfg(test)]\nmod tests {\n"
            + marker("fn tests::recur")
            + "fn recur() {}\n}\n"
        )
        names = [function.qualified_name for function in rust_functions(text)]
        self.assertEqual(
            names,
            [
                "<First<'a,T> as Trait<T>>::new",
                "Second::new",
                "outer",
                "outer::recur",
                "tests::recur",
            ],
        )
        self.assertEqual(check_rust("x.rs", text, self.entry(["fn " + n for n in names])), [])
        errors = check_rust("x.rs", text, self.entry(["fn new", "fn recur"]))
        self.assertTrue(any("'fn new' has no Rust declaration" in error for error in errors))
        self.assertTrue(any("'fn recur' has no Rust declaration" in error for error in errors))

    def test_trait_implementations_with_the_same_method_have_distinct_identities(self):
        text = (
            "impl From<First> for Error { fn from() {} }\n"
            "impl From<Second> for Error { fn from() {} }\n"
            "impl Error { fn from() {} }"
        )
        self.assertEqual(
            [f.qualified_name for f in rust_functions(text)],
            ["<Error as From<First>>::from", "<Error as From<Second>>::from", "Error::from"],
        )

    def test_module_regions_cover_nested_scopes_and_explicit_overrides(self):
        text = (
            header("mixed", [SOURCE, LOCAL])
            + marker("mod tests", "original", [LOCAL])
            + "mod tests {\n"
            + "fn setup() { fn nested() {} }\n"
            + "mod inner {\n"
            + marker("impl tests::inner::Fixture")
            + "impl Fixture {\nfn build() {}\n}\n"
            + marker("fn tests::inner::special")
            + "fn special() {}\n}\n}\n"
        )
        entry = self.entry(
            ["mod tests"],
            "original",
            [LOCAL],
            members={
                "mod tests": [
                    "fn tests::setup",
                    "fn tests::setup::nested",
                    "fn tests::inner::Fixture::build",
                    "fn tests::inner::special",
                ]
            },
        )
        entry["sources"] = [SOURCE, LOCAL]
        entry["parts"].append(
            {
                "items": ["impl tests::inner::Fixture", "fn tests::inner::special"],
                "origin": "adapted",
                "sources": [SOURCE],
                "members": {"impl tests::inner::Fixture": ["fn tests::inner::Fixture::build"]},
            }
        )
        self.assertEqual(check_rust("x.rs", text, entry), [])
        wrong_override = text.replace(
            "fn tests::inner::special | adapted", "fn tests::inner::special | original"
        )
        self.assertTrue(check_rust("x.rs", wrong_override, entry))
        missing_override = text.replace(marker("fn tests::inner::special"), "")
        self.assertTrue(check_rust("x.rs", missing_override, entry))

    def test_region_markers_require_exact_lexical_scope_and_ledger_identity(self):
        text = MIXED_HEADER + marker("mod tests") + "mod tests { fn inside() {} }\nfn outside() {}"
        entry = self.entry(["mod tests"], members={"mod tests": ["fn tests::inside"]})
        errors = check_rust("x.rs", text, entry)
        self.assertEqual(len(errors), 1)
        self.assertIn("fn outside has no mixed-file", errors[0])
        for item in ("mod wrong", "impl tests", "fn inside"):
            with self.subTest(item=item):
                self.assertTrue(check_rust("x.rs", text, self.entry([item])))
        external = MIXED_HEADER + marker("mod tests") + "mod tests;\nfn outside() {}"
        self.assertTrue(
            check_rust("x.rs", external, self.entry(["mod tests"], members={"mod tests": []}))
        )

    def test_region_members_require_well_formed_complete_unique_inventories(self):
        text = MIXED_HEADER + marker("mod tests") + "mod tests { fn inside() {} }"
        for members in (
            None,
            [],
            "fn tests::inside",
            {},
            {"mod tests": None},
            {"mod tests": "fn tests::inside"},
            {"mod tests": [None]},
            {"mod tests": [["fn tests::inside"]]},
            {"mod tests": ["mod tests"]},
            {"mod tests": ["fn "]},
            {"mod tests": ["fn tests::inside", "fn tests::inside"]},
            {"mod tests": []},
            {"mod tests": ["fn tests::inside", "fn tests::removed"]},
            {"mod tests": ["fn tests::inside"], "mod stale": []},
            {"mod tests": ["fn tests::inside"], "fn tests::inside": []},
        ):
            with self.subTest(members=members):
                entry = self.entry(["mod tests"])
                entry["parts"][0]["members"] = members
                errors = check_rust("x.rs", text, entry)
                self.assertTrue(any("members" in error for error in errors))
        entry = self.entry(["mod tests"])
        self.assertTrue(any("members" in error for error in check_rust("x.rs", text, entry)))

    def test_inventory_keys_must_belong_to_regions_in_the_same_part(self):
        text = (
            MIXED_HEADER
            + marker("mod tests")
            + "mod tests { fn inside() {} }\n"
            + marker("fn outside")
            + "fn outside() {}"
        )
        entry = self.entry(["mod tests"], members={"mod tests": ["fn tests::inside"]})
        entry["parts"].append(
            {
                "items": ["fn outside"],
                "origin": "adapted",
                "sources": [SOURCE],
                "members": {"mod tests": ["fn tests::inside"]},
            }
        )
        errors = check_rust("x.rs", text, entry)
        self.assertTrue(any("has no mod/impl item in this part" in error for error in errors))
        entry["parts"][1].pop("members")
        self.assertEqual(check_rust("x.rs", text, entry), [])

    def test_nested_regions_each_inventory_all_lexical_function_descendants(self):
        text = (
            MIXED_HEADER
            + marker("mod tests")
            + "mod tests {\n"
            + marker("impl tests::Fixture")
            + "impl Fixture { fn build() { fn nested() {} } }\n}"
        )
        descendants = ["fn tests::Fixture::build", "fn tests::Fixture::build::nested"]
        entry = self.entry(
            ["mod tests", "impl tests::Fixture"],
            members={"mod tests": descendants.copy(), "impl tests::Fixture": descendants.copy()},
        )
        self.assertEqual(check_rust("x.rs", text, entry), [])
        for region in ("mod tests", "impl tests::Fixture"):
            with self.subTest(region=region):
                changed = deepcopy(entry)
                changed["parts"][0]["members"][region].remove(descendants[-1])
                errors = check_rust("x.rs", text, changed)
                self.assertEqual(len(errors), 1)
                self.assertIn(f"region {region!r} members inventory missing", errors[0])

    def test_moving_or_removing_a_region_member_leaves_a_stale_inventory(self):
        text = MIXED_HEADER + marker("mod tests") + "mod tests { fn inside() {} }"
        entry = self.entry(["mod tests"], members={"mod tests": ["fn tests::inside"]})
        for changed in (
            text.replace("fn inside() {}", ""),
            text.replace("mod tests { fn inside() {} }", "mod tests {}\nfn inside() {}"),
        ):
            with self.subTest(text=changed):
                errors = check_rust("x.rs", changed, entry)
                self.assertTrue(any("members inventory has stale" in error for error in errors))

    def test_duplicate_rust_region_and_function_identities_fail_closed(self):
        text = MIXED_HEADER + marker("mod tests") + "mod tests { fn same() {} fn same() {} }"
        entry = self.entry(["mod tests"], members={"mod tests": ["fn tests::same"]})
        errors = check_rust("x.rs", text, entry)
        self.assertTrue(any("members inventory missing" in error for error in errors))
        text = (
            MIXED_HEADER
            + marker("impl Fixture")
            + "impl Fixture { fn first() {} }\n"
            + marker("impl Fixture")
            + "impl Fixture { fn second() {} }"
        )
        entry = self.entry(
            ["impl Fixture"],
            members={"impl Fixture": ["fn Fixture::first", "fn Fixture::second"]},
        )
        errors = check_rust("x.rs", text, entry)
        self.assertTrue(any("ambiguous Rust declaration identity" in error for error in errors))

    def test_own_function_assignment_does_not_cover_nested_functions(self):
        text = MIXED_HEADER + marker("fn outer") + "fn outer() { fn nested() {} }"
        errors = check_rust("x.rs", text, self.entry(["fn outer"]))
        self.assertEqual(len(errors), 1)
        self.assertIn("fn outer::nested has no mixed-file", errors[0])

    def test_nested_functions_in_array_and_call_argument_blocks_are_not_hidden(self):
        text = (
            MIXED_HEADER
            + marker("fn outer")
            + "fn outer() {\n"
            + "let _ = [ { fn array_helper() {} 1 } ];\n"
            + "consume({ fn argument_helper() {} 1 });\n"
            + "type Alias = impl Trait;\n}"
        )
        self.assertEqual(
            [item.identity for item in rust_items(text)],
            ["fn outer", "fn outer::array_helper", "fn outer::argument_helper"],
        )
        errors = check_rust("x.rs", text, self.entry(["fn outer"]))
        self.assertEqual(len(errors), 2)
        self.assertTrue(all("has no mixed-file provenance assignment" in e for e in errors))

    def test_an_own_marker_cannot_silently_override_a_region_without_a_ledger_item(self):
        text = (
            MIXED_HEADER
            + marker("mod tests")
            + "mod tests {\n"
            + marker("fn tests::x", "original")
            + "fn x() {} }"
        )
        errors = check_rust(
            "x.rs", text, self.entry(["mod tests"], members={"mod tests": ["fn tests::x"]})
        )
        self.assertTrue(any("provenance marker has no exact ledger item" in e for e in errors))

    def test_scope_metadata_tracks_modules_impls_nested_functions_and_boundaries(self):
        text = (
            "mod tests {\n// region\nimpl<T> Type<T> where T: Trait {\n"
            "fn outer(value: impl Trait) { fn nested() {} }\n}\n}\n"
            "trait Factory { fn make(); }\nmod external;\nfn after() {}"
        )
        items = rust_items(text)
        self.assertEqual(
            [item.identity for item in items],
            [
                "mod tests",
                "impl tests::Type",
                "fn tests::Type::outer",
                "fn tests::Type::outer::nested",
                "fn Factory::make",
                "mod external",
                "fn after",
            ],
        )
        self.assertEqual(items[3].ancestors, (0, 1, 2))
        self.assertEqual(items[3].parent, 2)
        self.assertIn("// region", items[1].prelude)
        self.assertEqual(text[items[1].start : items[1].start + 4], "impl")
        self.assertEqual(text[items[1].body_start], "{")
        self.assertEqual(text[items[1].end - 1], "}")
        self.assertIsNone(items[5].body_start)
        self.assertEqual(text[items[5].end - 1], ";")
        self.assertEqual(items[6].ancestors, ())

    def test_parts_must_be_well_formed_unique_and_use_header_sources(self):
        text = MIXED_HEADER + marker("fn x") + "fn x() {}"
        for change in ("duplicate", "missing-symbol", "foreign-source", "invalid-kind", "no-parts"):
            entry = self.entry(["fn x"])
            with self.subTest(change=change):
                if change == "duplicate":
                    entry["parts"].append(deepcopy(entry["parts"][0]))
                elif change == "missing-symbol":
                    entry["parts"][0]["sources"] = [
                        {k: v for k, v in SOURCE.items() if k != "symbol"}
                    ]
                elif change == "foreign-source":
                    entry["parts"][0]["sources"] = [LOCAL]
                elif change == "invalid-kind":
                    entry["parts"][0]["items"] = ["struct X"]
                else:
                    entry["parts"] = []
                self.assertTrue(check_rust("x.rs", text, entry))

    def test_invalid_ledger_origin_types_report_errors_without_crashing(self):
        text = MIXED_HEADER + marker("fn x") + "fn x() {}"
        entry = self.entry(["fn x"])
        entry["origin"] = ["mixed"]
        self.assertTrue(check_rust("x.rs", text, entry))
        entry = self.entry(["fn x"])
        entry["parts"][0]["origin"] = ["adapted"]
        self.assertTrue(check_rust("x.rs", text, entry))

    def test_missing_function_is_actionable_and_nonmixed_files_need_no_marker(self):
        errors = check_rust("x.rs", MIXED_HEADER, self.entry(["fn missing"]))
        self.assertTrue(any("'fn missing' has no Rust declaration" in e for e in errors))
        self.assertEqual(
            check_rust(
                "x.rs",
                header("port") + "fn x() {}",
                {"origin": "port", "license": "MIT", "sources": [SOURCE]},
            ),
            [],
        )

    def test_nonmixed_files_cannot_add_region_markers_without_mixed_file_parts(self):
        entry = {"origin": "original", "license": "MIT", "sources": [SOURCE]}
        text = HEADER + marker("mod tests", "original") + "mod tests { fn imported_case() {} }"
        errors = check_rust("x.rs", text, entry)
        self.assertTrue(
            any("markers require mixed-file origin and parts" in error for error in errors)
        )
        # A parts table alone cannot enable regions under a wholly-original header.
        entry["parts"] = [
            {
                "items": ["mod tests"],
                "origin": "original",
                "sources": [SOURCE],
                "members": {"mod tests": ["fn tests::imported_case"]},
            }
        ]
        self.assertTrue(check_rust("x.rs", text, entry))
        # Only actual line-comment fields count; examples inside Rust literals or
        # block comments do not turn a nonmixed file into a region assignment.
        for lookalike in (
            "/*\n" + marker("mod tests", "original") + "*/\n",
            'const EXAMPLE: &str = "// Provenance: mod tests | original";\n',
            'const EXAMPLE: &str = r#"\n' + marker("mod tests", "original") + '"#;\n',
            '#[doc = r"// Provenance: mod tests | original"]\n',
        ):
            with self.subTest(lookalike=lookalike):
                self.assertEqual(
                    check_rust("x.rs", HEADER + lookalike + "mod tests { fn local() {} }", entry),
                    [],
                )


if __name__ == "__main__":
    unittest.main()
