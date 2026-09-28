"""Nightly packages must identify their source without changing dependency locks."""

import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from prepare_nightly import stamp


class NightlyVersionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        source = Path(__file__).resolve().parents[1]
        for name in ("Cargo.toml", "Cargo.lock"):
            (self.root / name).write_bytes((source / name).read_bytes())

    def read(self, name):
        return tomllib.loads((self.root / name).read_text())

    def test_compiled_and_packaged_versions_match_without_dependency_changes(self):
        original_lock = self.read("Cargo.lock")
        package = self.read("Cargo.toml")["package"]
        version = stamp(self.root, identifier="20260928.12345.2")
        self.assertTrue(version.endswith("-nightly.20260928.12345.2"))
        self.assertEqual(self.read("Cargo.toml")["package"]["version"], version)
        for entry in original_lock["package"]:
            if entry["name"] == package["name"]:
                entry["version"] = version
        self.assertEqual(original_lock, self.read("Cargo.lock"))
        # All build jobs stamp exactly the version selected by the validation job.
        self.assertEqual(stamp(self.root, version=version), version)

    def test_mismatched_lockfile_changes_neither_file(self):
        lock = self.root / "Cargo.lock"
        lock.write_text(lock.read_text().replace('name = "reshiki"', 'name = "unrelated"'))
        before = [(self.root / name).read_bytes() for name in ("Cargo.toml", "Cargo.lock")]
        with self.assertRaises(ValueError):
            stamp(self.root, identifier="20260928.12345.1")
        self.assertEqual(
            before, [(self.root / name).read_bytes() for name in ("Cargo.toml", "Cargo.lock")]
        )

    def test_rejects_stable_or_other_release_versions_and_unsafe_identifiers(self):
        for version in ("0.9.1", "999.0.0-nightly.20260928.1.1", "$(touch bad)"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                stamp(self.root, version=version)
        for identifier in ("20260928.01.1", "20260928.1.0", "20260928/1/1", "20260928.1.1\n"):
            with self.subTest(identifier=identifier), self.assertRaises(ValueError):
                stamp(self.root, identifier=identifier)


if __name__ == "__main__":
    unittest.main()
