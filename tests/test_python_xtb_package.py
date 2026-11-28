"""The optional engine package locates only its own recorded executable."""

import importlib.util
import os
import sys
import tempfile
import unittest
from importlib.metadata import PackagePath
from pathlib import Path
from unittest.mock import Mock, patch

PACKAGE = Path(__file__).resolve().parents[1] / "crates/python-xtb/python/reshiki_xtb/__init__.py"


class XtbPackageTests(unittest.TestCase):
    def setUp(self):
        specification = importlib.util.spec_from_file_location("reshiki_xtb_test", PACKAGE)
        self.package = importlib.util.module_from_spec(specification)
        with patch("importlib.metadata.version", return_value="0.17.1"):
            specification.loader.exec_module(self.package)
        self.executable = "reshiki-xtb-worker" + (".exe" if sys.platform == "win32" else "")

    def test_version_comes_from_distribution_metadata(self):
        self.assertEqual(self.package.__version__, "0.17.1")

    def test_worker_is_found_through_the_distribution_file_record(self):
        with tempfile.TemporaryDirectory() as temporary:
            worker = Path(temporary) / "bin" / self.executable
            worker.parent.mkdir()
            worker.write_bytes(b"#!/bin/sh\n")
            worker.chmod(0o755)
            file = PackagePath(f"../../../bin/{self.executable}")
            installed = Mock(files=[PackagePath("reshiki_xtb/__init__.py"), file])
            installed.locate_file.return_value = worker
            with patch.object(self.package, "distribution", return_value=installed) as lookup:
                self.assertEqual(self.package.worker_path(), worker.resolve())
            lookup.assert_called_once_with("reshiki-xtb")
            installed.locate_file.assert_called_once_with(file)

    def test_relative_distribution_location_returns_an_absolute_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            worker = Path(temporary).resolve() / self.executable
            worker.write_bytes(b"#!/bin/sh\n")
            worker.chmod(0o755)
            installed = Mock(files=[PackagePath(worker.name)])
            installed.locate_file.return_value = Path(worker.name)
            with (
                patch.object(self.package, "distribution", return_value=installed),
                patch("os.getcwd", return_value=str(worker.parent)),
            ):
                self.assertEqual(self.package.worker_path(), worker.resolve())

    def test_missing_or_nonexecutable_worker_has_reinstall_instructions(self):
        with tempfile.TemporaryDirectory() as temporary:
            worker = Path(temporary) / self.executable
            for files, mode in (
                (None, 0o755),
                ([], 0o755),
                ([PackagePath("other/reshiki-xtb-worker-copy")], 0o755),
                ([PackagePath(f"bin/{self.executable}")], None),
                # Windows has no execute bit.
                *(
                    ()
                    if sys.platform == "win32"
                    else (([PackagePath(f"bin/{self.executable}")], 0o644),)
                ),
            ):
                worker.unlink(missing_ok=True)
                if mode is not None:
                    worker.write_bytes(b"#!/bin/sh\n")
                    worker.chmod(mode)
                installed = Mock(files=files)
                installed.locate_file.return_value = worker
                with (
                    self.subTest(files=files, mode=mode),
                    patch.object(self.package, "distribution", return_value=installed),
                    patch.dict(os.environ, {"PATH": str(worker.parent)}),
                    self.assertRaisesRegex(
                        FileNotFoundError,
                        "reinstall with: pip install --force-reinstall reshiki-xtb",
                    ),
                ):
                    self.package.worker_path()

    def test_windows_finds_only_the_recorded_exe_even_in_a_path_with_spaces(self):
        with tempfile.TemporaryDirectory(prefix="ReShiki wheel ") as temporary:
            worker = Path(temporary) / "reshiki-xtb-worker.exe"
            worker.write_bytes(b"MZ")
            worker.chmod(0o755)
            file = PackagePath("../../../Scripts/reshiki-xtb-worker.exe")
            installed = Mock(files=[PackagePath("bin/reshiki-xtb-worker"), file])
            installed.locate_file.return_value = worker
            with (
                patch.object(self.package, "distribution", return_value=installed),
                patch.object(self.package.sys, "platform", "win32"),
            ):
                self.assertEqual(self.package.worker_path(), worker.resolve())
            installed.locate_file.assert_called_once_with(file)


if __name__ == "__main__":
    unittest.main()
