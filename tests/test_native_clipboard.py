"""Test clipboard transport on a private pasteboard; leave the user's clipboard alone."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


@unittest.skipUnless(sys.platform == "darwin", "Native clipboard requires macOS")
class NativeClipboardTests(unittest.TestCase):
    def test_picture_formats_and_priorities_on_private_pasteboard(self):
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory(prefix="moruno-clipboard-tests-") as directory:
            binary = Path(directory) / "clipboard-tests"
            compiled = subprocess.run(
                [
                    "swiftc",
                    str(root / "native/macos/ClipboardSupport.swift"),
                    str(root / "tests/native_clipboard.swift"),
                    "-o",
                    str(binary),
                ],
                capture_output=True,
                text=True,
                timeout=60,
            )
            self.assertEqual(compiled.returncode, 0, compiled.stdout + compiled.stderr)
            result = subprocess.run([str(binary)], capture_output=True, text=True, timeout=15)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
