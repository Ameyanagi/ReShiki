"""Final wheels must match the generated license files and declare each one."""

import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from check_python_wheel_notices import verify_wheel


class PythonWheelNoticeTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.licenses = self.directory / "wheel-licenses"
        self.licenses.mkdir()
        self.files = {
            "LICENSE": b"Project terms\r\n",
            "THIRD-PARTY-NOTICES.txt": b"Dependency terms\n",
        }
        for name, content in self.files.items():
            (self.licenses / name).write_bytes(content)

    def wheel(self, *, files=None, declarations=None):
        files = self.files if files is None else files
        declarations = self.files if declarations is None else declarations
        path = self.directory / "reshiki-1.0-cp310-abi3-any.whl"
        metadata = "Metadata-Version: 2.4\nName: reshiki\nVersion: 1.0\n"
        metadata += "".join(f"License-File: wheel-licenses/{name}\n" for name in declarations)
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("reshiki-1.0.dist-info/METADATA", metadata + "\n")
            for name, content in files.items():
                archive.writestr(f"reshiki-1.0.dist-info/licenses/wheel-licenses/{name}", content)
        return path

    def test_matching_files_and_declarations_pass(self):
        verify_wheel(self.wheel(), self.licenses)

    def test_missing_generated_notices_are_rejected(self):
        (self.licenses / "THIRD-PARTY-NOTICES.txt").unlink()
        with self.assertRaisesRegex(ValueError, "No generated notices"):
            verify_wheel(self.wheel(files={}, declarations={}), self.licenses)

    def test_missing_or_extra_file_is_rejected(self):
        for name in self.files:
            files = {key: content for key, content in self.files.items() if key != name}
            with (
                self.subTest(missing=name),
                self.assertRaisesRegex(ValueError, "License files differ"),
            ):
                verify_wheel(self.wheel(files=files), self.licenses)
        with self.assertRaisesRegex(ValueError, "License files differ"):
            verify_wheel(
                self.wheel(files={**self.files, "extra.txt": b"Extra terms"}), self.licenses
            )

    def test_missing_license_file_declaration_is_rejected(self):
        for name in self.files:
            declarations = [key for key in self.files if key != name]
            with (
                self.subTest(file=name),
                self.assertRaisesRegex(ValueError, "Missing License-File"),
            ):
                verify_wheel(self.wheel(declarations=declarations), self.licenses)

    def test_differing_bytes_are_rejected(self):
        for name, content in self.files.items():
            files = {**self.files, name: content + b"\n"}
            with self.subTest(file=name), self.assertRaisesRegex(ValueError, "differing bytes"):
                verify_wheel(self.wheel(files=files), self.licenses)


if __name__ == "__main__":
    unittest.main()
