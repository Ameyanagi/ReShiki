"""Final wheel archives must preserve complete terms and their metadata entries."""

import contextlib
import io
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from check_python_wheel_notices import REQUIRED_FILES, SOURCE_FILES, verify_wheel, wheel_paths
from license_notices import PROJECT_FILES, THIRD_PARTY_FILE

ROOT = Path(__file__).resolve().parents[1]
OPSIN = b"""Copyright 2017 Daniel Lowe

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
"""


class PythonWheelNoticeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.notices = b"\n".join(
            relative.encode() + b"\n" + (ROOT / relative).read_bytes() for relative in SOURCE_FILES
        )
        self.notices += (
            b"\nDEPENDENCY: nazuchem-kernel@0.1.0\n"
            b"DEPENDENCY: nazuchem-n2s@0.1.0\n"
            b"https://github.com/dan2097/opsin\n" + OPSIN
        )

    def wheel(self, *, nested=True, omitted=(), declarations=None, notices=None, project=None):
        path = self.directory / "reshiki-1.0-cp310-abi3-any.whl"
        relative = "wheel-licenses/" if nested else ""
        payloads = {name: (ROOT / name).read_bytes() for name in PROJECT_FILES}
        payloads[THIRD_PARTY_FILE] = self.notices if notices is None else notices
        if project:
            payloads.update(project)
        if declarations is None:
            declarations = [relative + name for name in payloads]
        metadata = "Metadata-Version: 2.4\nName: reshiki\nVersion: 1.0\n"
        metadata += "".join(f"License-File: {name}\n" for name in declarations)
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("reshiki-1.0.dist-info/METADATA", metadata + "\n")
            for name, content in payloads.items():
                if name not in omitted:
                    archive.writestr(f"reshiki-1.0.dist-info/licenses/{relative}{name}", content)
        return path

    def test_complete_wheel_accepts_direct_and_nested_license_paths(self):
        for nested in (False, True):
            with self.subTest(nested=nested):
                verify_wheel(self.wheel(nested=nested))

    def test_each_required_file_must_be_in_the_final_archive(self):
        for name in REQUIRED_FILES:
            with self.subTest(file=name), self.assertRaisesRegex(ValueError, name):
                verify_wheel(self.wheel(omitted=(name,)))

    def test_optional_project_license_is_validated_when_present(self):
        declarations = [f"wheel-licenses/{name}" for name in REQUIRED_FILES]
        verify_wheel(self.wheel(omitted=("LICENSE",), declarations=declarations))
        with self.assertRaisesRegex(ValueError, "differs from repository: LICENSE$"):
            verify_wheel(self.wheel(project={"LICENSE": b"Project summary"}))

    def test_metadata_must_declare_every_required_license_path(self):
        declarations = [f"wheel-licenses/{name}" for name in PROJECT_FILES]
        with self.assertRaisesRegex(ValueError, "Missing METADATA License-File.*THIRD-PARTY"):
            verify_wheel(self.wheel(declarations=declarations))

    def test_metadata_cannot_point_to_missing_files_or_escape_license_directory(self):
        for declaration in ("missing.txt", "../LICENSE", "/LICENSE", "..\\LICENSE"):
            with self.subTest(path=declaration), self.assertRaisesRegex(ValueError, "License-File"):
                verify_wheel(self.wheel(declarations=[declaration]))

    def test_project_terms_must_match_the_repository_bytes(self):
        with self.assertRaisesRegex(ValueError, "differs from repository: LICENSE-MIT"):
            verify_wheel(self.wheel(project={"LICENSE-MIT": b"MIT summary"}))

    def test_source_licenses_and_notices_cannot_be_truncated(self):
        for relative in SOURCE_FILES:
            text = (ROOT / relative).read_bytes()
            truncated = self.notices.replace(text, text[: len(text) // 2])
            with (
                self.subTest(source=relative),
                self.assertRaisesRegex(ValueError, f"Missing complete source terms.*{relative}"),
            ):
                verify_wheel(self.wheel(notices=truncated))

    def test_source_attribution_paths_must_survive_consolidation(self):
        damaged = self.notices.replace(b"licenses/centres/NOTICE", b"unattributed")
        with self.assertRaisesRegex(ValueError, "Missing source attribution.*centres/NOTICE"):
            verify_wheel(self.wheel(notices=damaged))

    def test_opsin_needs_its_own_full_grant_disclaimer_and_copyright(self):
        unrelated_mit = (ROOT / "LICENSE-MIT").read_bytes()
        for incomplete in (
            OPSIN.split(b"THE SOFTWARE IS PROVIDED")[0],
            OPSIN.replace(b"Copyright 2017 Daniel Lowe", b"Copyright another author"),
            OPSIN.replace(b"subject to the following conditions:", b""),
        ):
            damaged = self.notices.replace(OPSIN, incomplete + unrelated_mit)
            with (
                self.subTest(opsin=incomplete[:60]),
                self.assertRaisesRegex(ValueError, "Missing complete OPSIN MIT license"),
            ):
                verify_wheel(self.wheel(notices=damaged))

    def test_naming_dependencies_and_upstream_attribution_are_required(self):
        for marker in (
            b"DEPENDENCY: nazuchem-kernel@",
            b"DEPENDENCY: nazuchem-n2s@",
            b"https://github.com/dan2097/opsin",
        ):
            with self.subTest(marker=marker), self.assertRaisesRegex(ValueError, "Missing"):
                verify_wheel(self.wheel(notices=self.notices.replace(marker, b"removed")))

    def test_quoted_globs_expand_and_overlapping_arguments_do_not_repeat_wheels(self):
        path = self.wheel()
        self.assertEqual(wheel_paths([str(self.directory / "*.whl"), str(path)]), [path])
        with self.assertRaisesRegex(ValueError, "No wheel files matched"):
            wheel_paths([str(self.directory / "missing-*.whl")])

    def test_cli_checks_quoted_globs_without_cargo(self):
        self.wheel()
        result = subprocess.run(
            [
                sys.executable,
                str(ROOT / "scripts/check_python_wheel_notices.py"),
                str(self.directory / "*.whl"),
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Verified complete license notices", result.stdout)

    def test_cli_rejects_unmatched_globs_clearly(self):
        from check_python_wheel_notices import main

        errors = io.StringIO()
        with contextlib.redirect_stderr(errors), self.assertRaises(SystemExit) as error:
            main([str(self.directory / "*.whl")])
        self.assertEqual(error.exception.code, 2)
        self.assertIn("No wheel files matched", errors.getvalue())


if __name__ == "__main__":
    unittest.main()
