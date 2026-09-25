"""Distribution regressions: nested notices, missing terms and pinned fallbacks."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from license_notices import PROJECT_FILES, copy_notices


class LicenseNoticeTests(unittest.TestCase):
    def test_nested_licenses_and_versioned_supplements_survive_packaging(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in PROJECT_FILES:
                (root / name).write_text(f"Project {name}")
            supplements = root / "licenses/rust"
            supplements.mkdir(parents=True)
            upstream = b"Upstream terms and original copyright"
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
                (source / "Cargo.toml").write_text('license = "MIT"')
                packages.append(
                    {
                        "name": name,
                        "version": "1.0",
                        "manifest_path": str(source / "Cargo.toml"),
                        "license": "MIT",
                    }
                )
            nested = root / "nested/vendor/library"
            nested.mkdir(parents=True)
            (nested / "NOTICE").write_text("Original nested attribution")
            output = root / "output"
            output.mkdir()
            copy_notices(root, output, {"packages": packages})
            self.assertEqual(
                (output / "rust/nested-1.0/vendor/library/NOTICE").read_text(),
                "Original nested attribution",
            )
            self.assertEqual(
                (output / "rust/omitted-1.0/upstream/terms.txt").read_bytes(), upstream
            )
            # A new version must not silently inherit a stale notice record.
            packages[1]["version"] = "2.0"
            with self.assertRaisesRegex(ValueError, "Missing license text"):
                copy_notices(root, output, {"packages": packages})
            packages[1]["version"] = "1.0"
            packages[1]["license"] = "BSD-3-Clause"
            with self.assertRaisesRegex(ValueError, "License changed"):
                copy_notices(root, output, {"packages": packages})
            packages[1]["license"] = "MIT"
            (supplements / "terms.txt").write_bytes(b"Accidental replacement")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                copy_notices(root, output, {"packages": packages})


if __name__ == "__main__":
    unittest.main()
