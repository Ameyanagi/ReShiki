"""Regressions for the optional pinned RDKit/Indigo reaction checker.

Run with: uv run --no-project --with rdkit==2026.9.2 --with epam.indigo==1.49.0
python -m unittest tests.test_chemdraw_reactions -v
"""

import importlib.util
import io
import tempfile
import unittest
from contextlib import redirect_stdout
from functools import partial
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "crates/io/tests/fixtures/reaction_steps"


@unittest.skipUnless(importlib.util.find_spec("indigo"), "Optional Indigo tool is required")
class ReactionCheckerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location(
            "chemdraw_reaction_checker", ROOT / "scripts/check_chemdraw_reactions.py"
        )
        cls.checker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.checker)
        cls.readers = (
            ("RDKit.CDXML", cls.checker.rdChemReactions.ReactionsFromCDXMLFile, (".cdxml",)),
            (
                "RDKit.ChemDraw",
                cls.checker.rdChemDraw.ReactionsFromChemDrawFile,
                (".cdx", ".cdxml"),
            ),
        )

    def check_mapped(self, path, expected):
        for reader, parse, extensions in self.readers:
            if path.suffix in extensions:
                with self.subTest(reader=reader, extension=path.suffix):
                    output = io.StringIO()
                    with redirect_stdout(output):
                        passed = self.checker.check(
                            path, reader, partial(self.checker.read_rdkit, parse)
                        )
                    self.assertEqual(passed, expected, output.getvalue())

    def test_mapped_fixture_keeps_exact_endpoints(self):
        for extension in (".cdx", ".cdxml"):
            self.check_mapped(FIXTURES / f"mapped{extension}", True)

    def test_mapped_fixture_rejects_swapped_or_missing_pairs(self):
        source = (FIXTURES / "mapped.cdxml").read_text()
        for original, replacement in (("3 15 4 14", "3 14 4 15"), (" 8 12", "")):
            with (
                self.subTest(mutation=(original, replacement)),
                tempfile.TemporaryDirectory() as tmp,
            ):
                self.assertIn(original, source)
                path = Path(tmp) / "mapped.cdxml"
                path.write_text(source.replace(original, replacement, 1))
                self.check_mapped(path, False)


if __name__ == "__main__":
    unittest.main()
