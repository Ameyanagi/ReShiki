"""Regression checks for release identity, archive naming, and secret handling."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from build_release import archive, check_tag, version
from sign_macos import is_macho, private_run


class ReleaseTests(unittest.TestCase):
    def test_tag_must_match_package_version(self):
        check_tag(f"v{version()}")
        with self.assertRaisesRegex(ValueError, "must match"):
            check_tag("v0.0.0-unrelated")

    def test_archive_name_preserves_all_version_components(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            folder = root / "moruno-1.2.3-macos-arm64"
            folder.mkdir()
            with (
                patch("build_release.platform.system", return_value="Darwin"),
                patch("build_release.run") as run,
            ):
                output = archive(folder, root / "output/moruno-1.2.3-macos-arm64")
            self.assertEqual(output.name, "moruno-1.2.3-macos-arm64.zip")
            self.assertIn("--keepParent", run.call_args.args[0])

    def test_signing_errors_never_include_credential_argv_or_output(self):
        result = subprocess.CompletedProcess([], 1, stdout="private-value", stderr="private-value")
        with patch("sign_macos.subprocess.run", return_value=result):
            with self.assertRaises(RuntimeError) as error:
                private_run(["security", "-p", "private-value"])
        self.assertNotIn("private-value", str(error.exception))

    def test_signing_detects_native_code_and_skips_symlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "library.so"
            binary.write_bytes(b"\xcf\xfa\xed\xfe\x00\x00")
            self.assertTrue(is_macho(binary))
            binary.write_bytes(b"text file")
            self.assertFalse(is_macho(binary))


if __name__ == "__main__":
    unittest.main()
