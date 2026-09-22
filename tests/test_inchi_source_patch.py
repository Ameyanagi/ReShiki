"""Checked build-copy repair must never mutate the pinned official sources."""

import copy
import hashlib
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import inchi_source_patch


class SourcePatchTests(unittest.TestCase):
    def test_private_copy_and_rejected_source_context_hash_and_path(self):
        original, changed = b"original text\n", b"repaired text\n"
        relative = "src/input.c"

        def digest(data):
            return hashlib.sha256(data).hexdigest()

        reference = {"inchi_version": "test", "files": {relative: digest(original)}}
        patch = {
            "inchi_version": "test",
            "files": {
                relative: {
                    "source_sha256": digest(original),
                    "patched_sha256": digest(changed),
                    "replacements": [{"before": "original", "after": "repaired", "count": 1}],
                }
            },
        }
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "official"
            (source / "src").mkdir(parents=True)
            (source / relative).write_bytes(original)
            staged = inchi_source_patch.stage_source(source, root / "build", reference, patch)
            self.assertEqual((staged / relative).read_bytes(), changed)
            self.assertEqual((source / relative).read_bytes(), original)
            # Restored build caches can contain hollow patched-source trees.
            # Each build must regenerate patches from verified original input.
            (staged / relative).unlink()
            fresh = inchi_source_patch.stage_source(source, root / "build", reference, patch)
            self.assertNotEqual(fresh, staged)
            self.assertEqual((fresh / relative).read_bytes(), changed)
            self.assertFalse((staged / relative).exists())
            self.assertEqual((source / relative).read_bytes(), original)
            for field, value in [("source_sha256", "bad"), ("patched_sha256", "bad")]:
                wrong = copy.deepcopy(patch)
                wrong["files"][relative][field] = value
                with self.assertRaises(ValueError):
                    inchi_source_patch.stage_source(source, root / "rejected", reference, wrong)
            wrong = copy.deepcopy(patch)
            wrong["files"][relative]["replacements"][0]["count"] = 2
            with self.assertRaisesRegex(ValueError, "context mismatch"):
                inchi_source_patch.stage_source(source, root / "rejected", reference, wrong)
            wrong = {**patch, "inchi_version": "other"}
            with self.assertRaisesRegex(ValueError, "version mismatch"):
                inchi_source_patch.stage_source(source, root / "rejected", reference, wrong)
            for path in ("../outside", "/outside", "src/../outside", "C:\\outside"):
                bad_reference = {**reference, "files": {path: digest(original)}}
                with self.assertRaises(ValueError):
                    inchi_source_patch.stage_source(
                        source, root / "rejected", bad_reference, {**patch, "files": {}}
                    )
            self.assertFalse((root / "rejected").exists())
            (source / relative).write_bytes(b"modified")
            with self.assertRaisesRegex(ValueError, "source changed"):
                inchi_source_patch.stage_source(source, root / "rejected", reference, patch)

    def test_native_stream_allocation_boundaries(self):
        binary = (
            ROOT
            / "artifacts/inchi-helper"
            / ("inchi-stream-test.exe" if os.name == "nt" else "inchi-stream-test")
        )
        if not binary.is_file():
            if os.environ.get("RESHIKI_REQUIRE_INCHI_HELPER"):
                self.fail("Build the pinned native development helper first")
            self.skipTest("Optional native stream test is not built")
        result = subprocess.run([binary], capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(b"stream allocation boundaries passed", result.stdout)


if __name__ == "__main__":
    unittest.main()
