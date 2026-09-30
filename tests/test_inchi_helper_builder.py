"""Target checks reject malformed and wrong-platform native helper headers."""

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "inchi_builder", ROOT / "scripts/build_inchi_helper.py"
)
assert SPEC is not None and SPEC.loader is not None
BUILDER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILDER)


class TargetTests(unittest.TestCase):
    def check_header(self, data, target):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "helper"
            path.write_bytes(data)
            self.assertEqual(BUILDER.verify_executable(path, target), target)
            for other in BUILDER.SUPPORTED_TARGETS:
                if other != target:
                    with self.assertRaises(ValueError):
                        BUILDER.verify_executable(path, other)

    def test_all_supported_target_headers(self):
        for machine, target in ((62, "x86_64"), (183, "aarch64")):
            header = bytearray(64)
            header[:6] = b"\x7fELF\x02\x01"
            header[18:20] = machine.to_bytes(2, "little")
            self.check_header(header, target + "-unknown-linux-gnu")
        header = bytearray(64)
        header[:4] = b"\xcf\xfa\xed\xfe"
        header[4:8] = (0x100000C).to_bytes(4, "little")
        self.check_header(header, "aarch64-apple-darwin")
        for machine, target in ((0x8664, "x86_64"), (0xAA64, "aarch64")):
            header = bytearray(70)
            header[:2] = b"MZ"
            header[60:64] = (64).to_bytes(4, "little")
            header[64:68] = b"PE\0\0"
            header[68:70] = machine.to_bytes(2, "little")
            self.check_header(header, target + "-pc-windows-msvc")

    def test_malformed_and_unsupported_headers(self):
        for data in (b"", b"MZ", b"\x7fELF", b"\xcf\xfa\xed\xfe", b"MZ" + b"\xff" * 62):
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "helper"
                path.write_bytes(data)
                with self.assertRaises(ValueError):
                    BUILDER.verify_executable(path, None)


class GitDependencyTests(unittest.TestCase):
    def test_release_metadata_rejects_unpinned_or_mismatched_sources(self):
        revision = "a" * 40
        url = "https://github.com/Ameyanagi/COSMolKit.git"
        source = f"git+{url}?rev={revision}#{revision}"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = (
                '[dependencies]\ncosmolkit-inchi = { version = "=0.3.0", '
                f'git = "{url}", rev = "{revision}" }}\n'
            )
            lock = (
                f'[[package]]\nname = "cosmolkit-inchi"\nversion = "0.3.0"\nsource = "{source}"\n'
            )
            (root / "Cargo.toml").write_text(manifest)
            (root / "Cargo.lock").write_text(lock)
            self.assertEqual(BUILDER.dependency(root)["revision"], revision)
            for invalid in (
                lock.replace("#" + revision, "#" + "b" * 40),
                lock.replace(f'source = "{source}"\n', ""),
            ):
                (root / "Cargo.lock").write_text(invalid)
                with self.assertRaisesRegex(ValueError, "locked Git revision"):
                    BUILDER.dependency(root)
            (root / "Cargo.lock").write_text(lock)
            for invalid in (
                manifest.replace(revision, revision[:7]),
                manifest.replace('rev = "' + revision + '"', 'branch = "main"'),
                manifest.replace('version = "=0.3.0"', 'version = "0.3"'),
            ):
                (root / "Cargo.toml").write_text(invalid)
                with self.assertRaisesRegex(ValueError, "full Git revision"):
                    BUILDER.dependency(root)


class RustBuildTests(unittest.TestCase):
    def test_production_targets_use_only_locked_cargo_build(self):
        for target in BUILDER.SUPPORTED_TARGETS:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)
                commands = []

                def run(command, **_kwargs):
                    commands.append(command)
                    self.assertIn(command[0], ("test-cargo", "test-rustc"))
                    stdout = ""
                    if command[0] == "test-rustc":
                        self.assertEqual(command[1:], ["-vV"])
                        stdout = "rustc test\nhost: " + target + "\n"
                    elif command[1] == "metadata":
                        stdout = json.dumps(dict(target_directory=str(output)))
                    else:
                        self.assertIn("--locked", command)
                        self.assertIn("--release", command)
                        self.assertEqual(
                            command[command.index("--bin") + 1], "reshiki-inchi-helper"
                        )
                        name = "reshiki-inchi-helper" + (".exe" if "windows" in target else "")
                        binary = output / target / "release" / name
                        binary.parent.mkdir(parents=True)
                        binary.write_bytes(b"test executable")
                    return subprocess.CompletedProcess(command, 0, stdout, "")

                with (
                    patch.object(
                        sys,
                        "argv",
                        [
                            "build_inchi_helper.py",
                            "--output",
                            temporary,
                            "--target",
                            target,
                            "--production",
                        ],
                    ),
                    patch.dict(
                        os.environ,
                        {
                            "CARGO": "test-cargo",
                            "RUSTC": "test-rustc",
                            "CC": "unavailable-cc",
                            "CXX": "unavailable-cxx",
                        },
                    ),
                    patch.object(BUILDER, "verify_executable", return_value=target),
                    patch.object(BUILDER.subprocess, "run", side_effect=run),
                    patch("builtins.print"),
                ):
                    BUILDER.main()
                metadata = json.loads((output / "build.json").read_text())
                self.assertTrue(metadata["production"])
                self.assertEqual(metadata["target"], target)
                self.assertEqual(metadata["dependency"]["name"], "cosmolkit-inchi")
                self.assertEqual(metadata["dependency"]["version"], "0.3.0")
                self.assertIn("src/chemistry/inchi/kernel.rs", metadata["source_hashes"])
                self.assertFalse(
                    any(str(arg).endswith((".c", ".cpp")) for cmd in commands for arg in cmd)
                )


if __name__ == "__main__":
    unittest.main()
