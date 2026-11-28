"""The artifact harness must receive bytes from the ZIP, not a later Cargo build."""

import contextlib
import hashlib
import io
import os
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from stage_xtb_smoke import main, stage_application


class PackagedSmokeTests(unittest.TestCase):
    def test_stages_only_the_exact_zip_application_with_its_digest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "release.zip"
            contents = b"MZ the original shipped application"
            with zipfile.ZipFile(archive, "w") as stream:
                stream.writestr("release/reshiki.exe", contents)
                stream.writestr("release/README.txt", "Not a companion runtime")
            binary, digest = stage_application(archive, root / "Staged app")
            self.assertEqual(binary.read_bytes(), contents)
            self.assertEqual(digest, hashlib.sha256(contents).hexdigest())
            self.assertEqual(list(binary.parent.iterdir()), [binary])

    def test_rejects_missing_or_ambiguous_application_entries(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for names in (["release/README.txt"], ["one/reshiki.exe", "two/reshiki.exe"]):
                with self.subTest(names=names):
                    archive = root / "release.zip"
                    with zipfile.ZipFile(archive, "w") as stream:
                        for name in names:
                            stream.writestr(name, b"contents")
                    with self.assertRaisesRegex(ValueError, "one application executable"):
                        stage_application(archive, root / "app")

    def test_cli_exports_the_zip_path_and_digest_for_the_next_ci_step(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "app-windows-x64.zip"
            contents = b"MZ shipped application"
            with zipfile.ZipFile(archive, "w") as stream:
                stream.writestr("release/reshiki.exe", contents)
            github_env = root / "github-env"
            with (
                patch.dict(os.environ, GITHUB_ENV=str(github_env), RUNNER_TEMP=str(root)),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                self.assertEqual(main([str(root / "*-windows-x64.zip")]), 0)
            values = dict(line.split("=", 1) for line in github_env.read_text().splitlines())
            binary = Path(values["RESHIKI_TEST_PACKAGED_APP"])
            self.assertEqual(binary.read_bytes(), contents)
            self.assertEqual(
                values["RESHIKI_TEST_PACKAGED_APP_SHA256"], hashlib.sha256(contents).hexdigest()
            )

    def test_cli_rejects_an_ambiguous_archive_glob(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ("one-windows-x64.zip", "two-windows-x64.zip"):
                (root / name).touch()
            with self.assertRaisesRegex(ValueError, "one shipped Windows ZIP"):
                main([str(root / "*-windows-x64.zip")])
