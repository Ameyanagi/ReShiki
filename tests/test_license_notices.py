"""Consolidated distribution: complete texts, attribution, deduplication and upgrades."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from license_notices import PROJECT_FILES, THIRD_PARTY_FILE, write_notices


class LicenseNoticeTests(unittest.TestCase):
    def test_nested_licenses_and_versioned_supplements_survive_packaging(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in PROJECT_FILES:
                (root / name).write_text(f"Project {name}")
            supplements = root / "licenses/rust"
            supplements.mkdir(parents=True)
            upstream = b"Upstream terms and original copyright\r\n"
            (supplements / "terms.txt").write_bytes(upstream)
            declaration = {
                "license": "MIT",
                "files": [{"path": "terms.txt", "sha256": hashlib.sha256(upstream).hexdigest()}],
            }
            (supplements / "manifest.json").write_text(json.dumps({"omitted@1.0": declaration}))
            packages = []
            for name in ["nested", "omitted"]:
                source = root / name
                source.mkdir()
                (source / "Cargo.toml").write_text(
                    'license = "MIT"\nauthors = ["Original Author"]\n'
                )
                packages.append(
                    {
                        "name": name,
                        "version": "1.0",
                        "manifest_path": str(source / "Cargo.toml"),
                        "license": "MIT",
                        "authors": ["Original Author"],
                    }
                )
            nested = root / "nested/vendor/library"
            nested.mkdir(parents=True)
            (nested / "NOTICE").write_text("Original nested attribution")
            output = root / "output"
            (output / "rust/legacy").mkdir(parents=True)
            (output / "rust/legacy/LICENSE").write_text("old")
            (output / "sources").mkdir()
            (output / "sources/old").write_text("old")
            (output / "rust-dependencies.json").write_text("{}")
            write_notices(root, output, {"packages": packages})
            content = (output / THIRD_PARTY_FILE).read_bytes()
            self.assertIn(b"nested@1.0/vendor/library/NOTICE", content)
            self.assertIn(b"Original nested attribution", content)
            self.assertIn(b"omitted@1.0/upstream/terms.txt", content)
            self.assertIn(upstream, content)
            self.assertIn(b"Original Author", content)
            self.assertEqual({p.name for p in output.iterdir()}, {*PROJECT_FILES, THIRD_PARTY_FILE})
            for name in PROJECT_FILES:
                self.assertEqual((root / name).read_bytes(), (output / name).read_bytes())
            # A new version must not silently inherit a stale notice record.
            packages[1]["version"] = "2.0"
            with self.assertRaisesRegex(ValueError, "Missing license text"):
                write_notices(root, output, {"packages": packages})
            self.assertEqual((output / THIRD_PARTY_FILE).read_bytes(), content)
            packages[1]["version"] = "1.0"
            packages[1]["license"] = "BSD-3-Clause"
            with self.assertRaisesRegex(ValueError, "License changed"):
                write_notices(root, output, {"packages": packages})
            packages[1]["license"] = "MIT"
            (supplements / "terms.txt").write_bytes(b"Accidental replacement")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                write_notices(root, output, {"packages": packages})

    def test_identical_texts_are_shared_without_merging_different_copyrights(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in PROJECT_FILES:
                (root / name).write_text(f"Project {name}")
            (root / "licenses/rust").mkdir(parents=True)
            (root / "licenses/rust/manifest.json").write_text("{}")
            shared = b"Full license terms, including the disclaimer.\n"
            packages = []
            for name in ["one", "two"]:
                source = root / name
                source.mkdir()
                (source / "Cargo.toml").write_text(f'name = "{name}"\nlicense = "MIT"\n')
                (source / "LICENSE").write_bytes(shared)
                (source / "COPYRIGHT").write_text(f"Copyright {name}")
                packages.append(
                    {
                        "name": name,
                        "version": "1",
                        "license": "MIT",
                        "manifest_path": str(source / "Cargo.toml"),
                    }
                )
            write_notices(root, root / "output", {"packages": packages})
            result = (root / "output" / THIRD_PARTY_FILE).read_bytes()
            digest = hashlib.sha256(shared).hexdigest().encode()
            self.assertEqual(result.count(b"BEGIN TEXT SHA-256 " + digest), 1)
            self.assertEqual(result.count(b"Text SHA-256: " + digest), 2)
            self.assertIn(b"one@1/LICENSE", result)
            self.assertIn(b"two@1/LICENSE", result)
            self.assertIn(b"Copyright one", result)
            self.assertIn(b"Copyright two", result)
            self.assertEqual(result.count(shared), 1)
            # Output is deterministic regardless of metadata package order.
            write_notices(root, root / "again", {"packages": list(reversed(packages))})
            self.assertEqual((root / "again" / THIRD_PARTY_FILE).read_bytes(), result)


if __name__ == "__main__":
    unittest.main()
