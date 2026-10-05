"""Reference-dependent integration targets must opt into the development feature."""

import re
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
# Fixture-only replays that need no interpreter run in default builds.
MARKERS = (".venv", "PythonEngine", "mod cip_rule_case")
PATH_LITERAL = re.compile(r'"((?:tests|reference)/[A-Za-z0-9_./-]+\.(?:py|cpp|h|md|rs))"')


class ReferenceFeatures(unittest.TestCase):
    def test_oracle_targets_require_the_reference_feature(self):
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
        self.assertIn("rdkit-reference", manifest["features"])
        self.assertNotIn("rdkit-reference", manifest["features"].get("default", []))
        configured = {item["name"]: item for item in manifest.get("test", [])}
        observed = set()
        for path in (ROOT / "reference").glob("*.rs"):
            source = path.read_text(encoding="utf-8")
            if any(marker in source for marker in MARKERS):
                observed.add(path.stem)
                with self.subTest(target=path.stem):
                    self.assertIn(path.stem, configured)
                    self.assertIn(
                        "rdkit-reference",
                        configured.get(path.stem, {}).get("required-features", []),
                    )
        self.assertGreaterEqual(len(observed), 85)
        for path in (ROOT / "tests").glob("*.rs"):
            source = path.read_text(encoding="utf-8")
            with self.subTest(native=path.stem):
                self.assertFalse(
                    any(marker in source for marker in MARKERS),
                    "reference-dependent targets belong in reference/",
                )
        for name, item in configured.items():
            if "rdkit-reference" in item.get("required-features", []):
                with self.subTest(path=name):
                    self.assertEqual(item.get("path"), f"reference/{name}.rs")

    def test_reference_path_literals_exist(self):
        sources = [
            *(ROOT / "reference").rglob("*.rs"),
            ROOT / "src/engine/reference.rs",
            *(ROOT / "src/engine/reference").rglob("*.rs"),
            ROOT / "src/chemistry/cleanup/numeric.rs",
            *(ROOT / "tests").glob("*.rs"),
            *(ROOT / "scripts").glob("*.py"),
        ]
        checked = 0
        for source in sources:
            for literal in PATH_LITERAL.findall(source.read_text(encoding="utf-8")):
                checked += 1
                with self.subTest(source=str(source.relative_to(ROOT)), literal=literal):
                    self.assertTrue((ROOT / literal).is_file())
        self.assertGreater(checked, 0)


if __name__ == "__main__":
    unittest.main()
