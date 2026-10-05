"""The reference compiler must see Git's pinned source bytes on every host."""

import hashlib
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from setup_windows_depict_reference import initialize_checkout


class ReferenceCheckoutTests(unittest.TestCase):
    def test_explicit_text_attributes_keep_lf_under_a_crlf_git_default(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = root / "global.gitconfig"
            config.write_text("[core]\n    autocrlf = false\n    eol = crlf\n")
            with patch.dict(os.environ, {"GIT_CONFIG_GLOBAL": str(config)}):
                source, destination = root / "source", root / "reference"
                subprocess.run(["git", "init", "--quiet", str(source)], check=True)
                (source / ".gitattributes").write_bytes(b"*.cpp text\n")
                original = b"double source_value() {\n  return 1.0;\n}\n"
                (source / "reference.cpp").write_bytes(original)
                subprocess.run(["git", "-C", str(source), "add", "."], check=True)
                subprocess.run(
                    [
                        "git",
                        "-C",
                        str(source),
                        "-c",
                        "user.name=Fixture",
                        "-c",
                        "user.email=fixture@example.invalid",
                        "-c",
                        "commit.gpgsign=false",
                        "commit",
                        "--quiet",
                        "-m",
                        "Pinned source",
                    ],
                    check=True,
                )
                revision = subprocess.check_output(
                    ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
                ).strip()
                initialize_checkout(destination)
                subprocess.run(
                    ["git", "-C", str(destination), "fetch", "--quiet", str(source), revision],
                    check=True,
                )
                subprocess.run(
                    [
                        "git",
                        "-C",
                        str(destination),
                        "checkout",
                        "--quiet",
                        "--detach",
                        "FETCH_HEAD",
                    ],
                    check=True,
                )
                pinned = subprocess.check_output(
                    ["git", "-C", str(destination), "show", f"{revision}:reference.cpp"]
                )
                self.assertEqual(pinned, original)
                actual = (destination / "reference.cpp").read_bytes()
                self.assertEqual(actual, pinned)
                self.assertEqual(hashlib.sha256(actual).digest(), hashlib.sha256(pinned).digest())


if __name__ == "__main__":
    unittest.main()
