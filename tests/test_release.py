"""Regression checks for release identity, archive naming, and secret handling."""

import contextlib
import hashlib
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from build_release import (
    archive,
    check_tag,
    main,
    notices,
    release_platform,
    verify_binary,
    verify_interpreter,
    version,
)
from sign_macos import is_macho, private_run


class ReleaseTests(unittest.TestCase):
    def test_release_includes_adapted_source_licenses(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "Licenses"
            with patch("build_release.run") as run:
                run.side_effect = lambda *args, **kwargs: kwargs["stdout"].write('{"packages": []}')
                notices(destination)
            self.assertIn(
                "BSD 3-Clause License",
                (destination / "sources/rdkit/LICENSE").read_text(),
            )
            self.assertIn(
                "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
                (destination / "sources/rdkit/NOTICE").read_text(),
            )

    @staticmethod
    def pe_image(machine):
        header = bytearray(64)
        header[:2] = b"MZ"
        header[60:64] = (64).to_bytes(4, "little")
        return header + b"PE\0\0" + machine.to_bytes(2, "little")

    def test_windows_arm_package_uses_rust_target_under_x64_python(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = "aarch64-pc-windows-msvc"
            binary = root / "target" / target / "release/reshiki.exe"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(self.pe_image(0xAA64))
            worker = root / "worker"
            worker.mkdir()
            (worker / "pyproject.toml").write_text("fixture")
            with (
                patch("build_release.ROOT", root),
                patch("build_release.version", return_value="1.2.3"),
                patch("build_release.platform.system", return_value="Windows"),
                patch("build_release.platform.machine", return_value="AMD64"),
                patch("build_release.runtime_project", return_value=worker),
                patch("build_release.target_directory", return_value=root / "target"),
                patch("build_release.notices"),
                patch("build_release.verify_archive"),
                patch("build_release.run") as run,
                patch("sys.argv", ["build_release.py", "--target", target]),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                main()
            run.assert_called_once_with(
                ["cargo", "build", "--release", "--locked", "--target", target], cwd=root
            )
            package = root / "dist/releases/reshiki-1.2.3-windows-arm64.zip"
            checksum = Path(str(package) + ".sha256").read_bytes()
            digest = hashlib.sha256(package.read_bytes()).hexdigest()
            # The Linux publisher must be able to verify Windows-generated manifests.
            self.assertEqual(checksum, f"{digest}  {package.name}\n".encode("ascii"))
            with zipfile.ZipFile(package) as stream:
                metadata = json.loads(stream.read("reshiki-1.2.3-windows-arm64/build.json"))
                self.assertEqual(metadata["architecture"], "arm64")
                self.assertEqual(metadata["rust_target"], target)
                self.assertEqual(metadata["chemistry_architecture"], "x64")
                self.assertFalse(metadata["signed"])
                self.assertIn(
                    b"Windows 11 on ARM is required",
                    stream.read("reshiki-1.2.3-windows-arm64/README.txt"),
                )

    def test_native_headers_reject_mislabeled_or_damaged_archives(self):
        elf = bytearray(64)
        elf[:6] = b"\x7fELF\x02\x01"
        elf[18:20] = (183).to_bytes(2, "little")
        fixtures = [
            ("windows", self.pe_image(0xAA64)),
            ("linux", elf),
            ("macos", b"\xcf\xfa\xed\xfe" + (0x0100000C).to_bytes(4, "little")),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "native"
            for system, data in fixtures:
                with self.subTest(system=system):
                    binary.write_bytes(data)
                    verify_binary(binary, system, "arm64")
                    with self.assertRaisesRegex(ValueError, "Expected .* x64 executable"):
                        verify_binary(binary, system, "x64")
                    binary.write_bytes(data[:4])
                    with self.assertRaises(ValueError):
                        verify_binary(binary, system, "arm64")

    def test_release_targets_do_not_include_intel_mac_or_guess_unknown_architectures(self):
        self.assertEqual(release_platform("aarch64-unknown-linux-gnu"), ("linux", "arm64"))
        for target in ["x86_64-apple-darwin", "riscv64gc-unknown-linux-gnu"]:
            with self.assertRaisesRegex(ValueError, "Unsupported release target"):
                release_platform(target)

    def test_interpreter_check_uses_the_running_python_not_the_uv_launcher(self):
        fixtures = [
            (
                "windows",
                "x64",
                {"system": "Windows", "machine": "ARM64", "platform": "win-amd64", "bits": 64},
            ),
            (
                "macos",
                "arm64",
                {
                    "system": "Darwin",
                    "machine": "arm64",
                    "platform": "macosx-11.0-universal2",
                    "bits": 64,
                },
            ),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            launcher = Path(temporary) / "python"
            # A launcher is not necessarily the actual interpreter executable.
            launcher.write_text("launcher fixture")
            for system, architecture, details in fixtures:
                with self.subTest(system=system):
                    response = subprocess.CompletedProcess([], 0, stdout=json.dumps(details))
                    with patch("build_release.run", return_value=response) as run:
                        verify_interpreter(launcher, system, architecture)
                        self.assertEqual(run.call_args.args[0][0:2], [launcher, "-c"])
                        wrong_arch = "arm64" if architecture == "x64" else "x64"
                        with self.assertRaisesRegex(ValueError, "Python interpreter"):
                            verify_interpreter(launcher, system, wrong_arch)

    def test_tag_must_match_package_version(self):
        check_tag(f"v{version()}")
        with self.assertRaisesRegex(ValueError, "must match"):
            check_tag("v0.0.0-unrelated")

    def test_archive_name_preserves_all_version_components(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            folder = root / "reshiki-1.2.3-macos-arm64"
            folder.mkdir()
            with (
                patch("build_release.platform.system", return_value="Darwin"),
                patch("build_release.run") as run,
            ):
                output = archive(folder, root / "output/reshiki-1.2.3-macos-arm64")
            self.assertEqual(output.name, "reshiki-1.2.3-macos-arm64.zip")
            self.assertIn("--keepParent", run.call_args.args[0])

    def test_windows_archive_accepts_reproducible_wheel_timestamps(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            folder = root / "reshiki-1.2.3-windows-x64"
            folder.mkdir()
            library = folder / "runtime.dll"
            library.write_bytes(b"library")
            os.utime(library, (1, 1))
            with patch("build_release.platform.system", return_value="Windows"):
                output = archive(folder, root / "output" / folder.name)
            with zipfile.ZipFile(output) as stream:
                self.assertEqual(stream.read(folder.name + "/runtime.dll"), b"library")
                self.assertEqual(stream.getinfo(folder.name + "/runtime.dll").date_time[0], 1980)

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
