"""Exercise the fixed native arena independently of chemistry or Rust."""

import os
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ARENA = (
    ROOT
    / "artifacts/inchi-helper"
    / ("inchi-arena-test.exe" if os.name == "nt" else "inchi-arena-test")
)


class ArenaTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not ARENA.is_file():
            if os.environ.get("RESHIKI_REQUIRE_INCHI_HELPER"):
                raise RuntimeError("Build the pinned native development helper first")
            raise unittest.SkipTest("Optional native allocator tests are not built")

    def test_allocation_reallocation_alignment_coalescing_and_lifecycle(self):
        result = subprocess.run([str(ARENA), "stress"], capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(b"capacity=1048576", result.stdout)

    def test_tiny_overflow_fragmented_and_oversized_budgets(self):
        for mode in ("tiny", "overflow", "huge", "fragmentation", "strdup"):
            with self.subTest(mode=mode):
                result = subprocess.run([str(ARENA), mode], capture_output=True, timeout=3)
                self.assertEqual(result.returncode, 21, result.stdout + result.stderr)
                self.assertIn(b"reason=1", result.stdout)


if __name__ == "__main__":
    unittest.main()
