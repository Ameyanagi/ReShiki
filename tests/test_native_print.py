"""Exercise the real macOS print renderer using Save to PDF, never a printer job."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


@unittest.skipUnless(sys.platform == "darwin", "Native printing requires macOS")
class NativePrintingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="reshiki-print-build-")
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.root = Path(__file__).resolve().parents[1]
        cls.directory = Path(cls.temporary.name)
        result = subprocess.run(
            [
                os.environ.get("CARGO", "cargo"),
                "test",
                "--locked",
                "-p",
                "reshiki-macos",
                "--test",
                "native_print",
                "--no-run",
                "--message-format=json",
            ],
            cwd=cls.root,
            capture_output=True,
            text=True,
            timeout=300,
        )
        if result.returncode:
            raise RuntimeError(result.stdout + result.stderr)
        artifacts = [
            json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")
        ]
        cls.renderer = next(
            Path(item["executable"])
            for item in artifacts
            if item.get("reason") == "compiler-artifact"
            and item.get("target", {}).get("name") == "native_print"
            and item.get("executable")
        )

    def test_physical_pages_scale_positions_and_page_ranges(self):
        result = subprocess.run([str(self.renderer)], capture_output=True, text=True, timeout=300)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_malformed_requests_exit_without_showing_a_dialog(self):
        invalid = self.directory / "invalid.pdf"
        invalid.write_text("This is not a PDF")
        requests = [
            b"{}",
            b"x" * 65537,
            json.dumps({"path": str(invalid), "title": "Invalid"}).encode(),
            json.dumps({"path": str(self.directory / "missing.pdf"), "title": "Missing"}).encode(),
            json.dumps({"path": str(invalid), "title": "x" * 1025}).encode(),
        ]
        for request in requests:
            with self.subTest(length=len(request)):
                result = subprocess.run(
                    [str(self.renderer), "--worker"], input=request, capture_output=True, timeout=10
                )
                self.assertEqual(result.returncode, 1, result.stderr.decode(errors="replace"))
                self.assertEqual(result.stdout, b"")
                self.assertLess(len(result.stderr), 1024)


if __name__ == "__main__":
    unittest.main()
