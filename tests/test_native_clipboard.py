"""Exercise Rust clipboard transport on a private pasteboard."""

import os
import subprocess
import sys
import unittest
from pathlib import Path


@unittest.skipUnless(sys.platform == "darwin", "Native clipboard requires macOS")
class NativeClipboardTests(unittest.TestCase):
    def test_picture_formats_and_priorities_on_private_pasteboard(self):
        root = Path(__file__).resolve().parents[1]
        result = subprocess.run(
            [
                os.environ.get("CARGO", "cargo"),
                "test",
                "--locked",
                "-p",
                "reshiki-macos",
                "--lib",
                "clipboard::tests::",
            ],
            cwd=root,
            capture_output=True,
            text=True,
            timeout=300,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("1 passed", result.stdout)


if __name__ == "__main__":
    unittest.main()
