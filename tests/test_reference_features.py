"""Reference-dependent integration targets must opt into the development feature."""

import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class ReferenceFeatures(unittest.TestCase):
    def test_oracle_targets_require_the_reference_feature(self):
        manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertIn("rdkit-reference", manifest["features"])
        self.assertNotIn("rdkit-reference", manifest["features"].get("default", []))
        configured = {item["name"]: item for item in manifest.get("test", [])}
        observed = set()
        for path in (ROOT / "tests").glob("*.rs"):
            source = path.read_text()
            if any(marker in source for marker in (".venv", "PythonEngine", "mod cip_rule_case")):
                observed.add(path.stem)
                with self.subTest(target=path.stem):
                    self.assertIn(path.stem, configured)
                    self.assertIn(
                        "rdkit-reference",
                        configured.get(path.stem, {}).get("required-features", []),
                    )
        self.assertGreaterEqual(len(observed), 88)


if __name__ == "__main__":
    unittest.main()
