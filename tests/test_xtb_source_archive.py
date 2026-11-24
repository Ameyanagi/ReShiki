"""Unit checks for offline xTB source packaging, without compiling the application."""

import contextlib
import gzip
import hashlib
import io
import json
import os
import shlex
import sys
import tarfile
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from xtb_source_archive import (
    FIXED_MTIME,
    MANIFEST,
    build_instructions,
    check,
    exclude_axtb_source,
    file_hashes,
    merge_vendor_config,
    modify_driver_version,
    patch_arguments,
    verify_checksum,
    verify_manifest,
    write_archive,
    write_manifest,
)

ROOT = Path(__file__).resolve().parents[1]
CRATES = {
    "axtb": "axtb",
    "axtb-base": "base",
    "axtb-d4": "d4",
    "axtb-driver": "driver",
    "axtb-gfnff": "gfnff",
    "axtb-gxtb": "gxtb",
    "axtb-solvation": "solvation",
}


def fork_fixture(root):
    for name, directory in CRATES.items():
        path = root / "crates" / directory / "Cargo.toml"
        path.parent.mkdir(parents=True)
        path.write_text(f'[package]\nname = "{name}"\nversion = "0.2.0"\n', encoding="utf-8")
    response = root / "crates/driver/src/response.rs"
    response.parent.mkdir()
    response.write_text('pub const VERSION: &str = env!("CARGO_PKG_VERSION");\n', encoding="utf-8")


class XtbSourceArchiveTests(unittest.TestCase):
    def test_patch_arguments_cover_exactly_seven_real_crate_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "axtb-source"
            fork_fixture(source)
            arguments = patch_arguments(source)
            self.assertEqual(arguments[::2], ["--config"] * 7)
            document = tomllib.loads("\n".join(arguments[1::2]))
            patches = document["patch"]["https://github.com/Ameyanagi/axtb"]
            self.assertEqual(set(patches), set(CRATES))
            for name, directory in CRATES.items():
                expected = f"axtb-source/crates/{directory}"
                self.assertEqual(patches[name], {"path": expected})
                self.assertTrue((source.parent / expected / "Cargo.toml").is_file())

    def test_patch_arguments_reject_missing_or_wrong_crate(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            fork_fixture(source)
            manifest = source / "crates/base/Cargo.toml"
            manifest.write_text('[package]\nname = "wrong-crate"\n', encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "Expected crate axtb-base"):
                patch_arguments(source)
            manifest.unlink()
            with self.assertRaisesRegex(ValueError, "Missing axtb crate manifest"):
                patch_arguments(source)

    def test_staged_workspace_excludes_the_entire_fork(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / "Cargo.toml"
            original = '[workspace]\nmembers = ["crates/worker"]\n'
            manifest.write_text(original, encoding="utf-8")
            exclude_axtb_source(root)
            staged = manifest.read_text(encoding="utf-8")
            self.assertEqual(
                tomllib.loads(staged)["workspace"],
                {"members": ["crates/worker"], "exclude": ["axtb-source"]},
            )
            exclude_axtb_source(root)
            self.assertEqual(manifest.read_text(encoding="utf-8"), staged)

    def test_manifest_hashes_every_file_and_detects_changes_missing_and_extra_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "source").mkdir()
            original = root / "source/main.rs"
            original.write_bytes(b"fn main() {}\n")
            (root / "Cargo.lock").write_bytes(b"locked dependency graph\n")
            details = {"reshiki": {"commit": "a" * 40}, "axtb": {"rev": "b" * 40}}
            manifest = write_manifest(root, details)
            expected = {
                "Cargo.lock": hashlib.sha256(b"locked dependency graph\n").hexdigest(),
                "source/main.rs": hashlib.sha256(b"fn main() {}\n").hexdigest(),
            }
            self.assertEqual(manifest["files"], expected)
            self.assertEqual(verify_manifest(root), {**details, "files": expected})
            self.assertNotIn(MANIFEST, file_hashes(root))
            original.write_bytes(b"changed\n")
            with self.assertRaisesRegex(ValueError, "source/main.rs"):
                verify_manifest(root)
            original.unlink()
            with self.assertRaisesRegex(ValueError, "source/main.rs"):
                verify_manifest(root)
            original.write_bytes(b"fn main() {}\n")
            (root / "unexpected").touch()
            with self.assertRaisesRegex(ValueError, "unexpected"):
                verify_manifest(root)

    def test_manifest_hashes_symlink_target_without_following_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "original").write_bytes(b"original")
            try:
                (root / "link").symlink_to("original")
            except OSError as error:
                if getattr(error, "winerror", None) != 1314:
                    raise
                self.skipTest("Windows requires privileges to create a symbolic link")
            write_manifest(root, {})
            self.assertEqual(file_hashes(root)["link"], hashlib.sha256(b"original").hexdigest())
            (root / "link").unlink()
            (root / "link").symlink_to("another-file")
            with self.assertRaisesRegex(ValueError, "link"):
                verify_manifest(root)

    def test_vendor_config_preserves_existing_env_and_relative_source_replacements(self):
        existing = (
            '[env]\n# Keep the system compiler.\nCC_aarch64_apple_darwin = "/usr/bin/clang"\n'
        )
        vendor = """[source.crates-io]
replace-with = "vendored-sources"

[source."git+https://github.com/Ameyanagi/axtb?rev=abc"]
git = "https://github.com/Ameyanagi/axtb"
rev = "abc"
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendored-crates"
"""
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / ".cargo/config.toml"
            path.parent.mkdir()
            path.write_text(existing, encoding="utf-8")
            merge_vendor_config(path, vendor)
            merged = path.read_text(encoding="utf-8")
            self.assertTrue(merged.startswith(existing))
            parsed = tomllib.loads(merged)
            self.assertEqual(parsed["env"], {"CC_aarch64_apple_darwin": "/usr/bin/clang"})
            self.assertEqual(parsed["source"]["vendored-sources"], {"directory": "vendored-crates"})
            self.assertEqual(parsed["source"]["crates-io"]["replace-with"], "vendored-sources")
            with self.assertRaises(tomllib.TOMLDecodeError):
                merge_vendor_config(path, vendor)
            self.assertEqual(path.read_text(encoding="utf-8"), merged)

    def test_archive_entries_are_sorted_and_have_fixed_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            temporary = Path(temporary)
            root = temporary / "reshiki-fixture-source"
            (root / "z").mkdir(parents=True)
            (root / "a").write_bytes(b"first\n")
            (root / "z-extra").touch()
            executable = root / "z/run.sh"
            executable.write_bytes(b"#!/bin/sh\n")
            executable.chmod(0o755)
            archive = temporary / "first.tar.gz"
            write_archive(root, archive)
            verify_checksum(archive)
            with tarfile.open(archive) as stream:
                entries = stream.getmembers()
                self.assertEqual(
                    [entry.name for entry in entries], sorted(entry.name for entry in entries)
                )
                for entry in entries:
                    self.assertEqual((entry.mtime, entry.uid, entry.gid), (FIXED_MTIME, 0, 0))
                    self.assertEqual((entry.uname, entry.gname), ("", ""))
                    self.assertEqual(entry.pax_headers, {})
                self.assertEqual(stream.getmember(f"{root.name}/a").mode, 0o644)
                self.assertEqual(
                    stream.getmember(f"{root.name}/z/run.sh").mode,
                    0o755 if executable.stat().st_mode & 0o111 else 0o644,
                )
            self.assertEqual(int.from_bytes(archive.read_bytes()[4:8], "little"), FIXED_MTIME)
            with gzip.open(archive) as contents:
                self.assertTrue(contents.read())
            os.utime(executable, (123456, 123456))
            second = temporary / "different-filename.tar.gz"
            write_archive(root, second)
            self.assertEqual(archive.read_bytes(), second.read_bytes())
            archive.write_bytes(b"corrupted archive")
            with self.assertRaisesRegex(ValueError, "Archive SHA-256 mismatch"):
                verify_checksum(archive)

    def test_rebuild_instructions_match_toolchain_prerequisites_and_patch_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fork_fixture(root / "axtb-source")
            (root / "docs").mkdir()
            for relative in ("rust-toolchain.toml", "docs/development.md"):
                (root / relative).write_bytes((ROOT / relative).read_bytes())
            instructions = build_instructions(root)
            toolchain = tomllib.loads((root / "rust-toolchain.toml").read_text())["toolchain"][
                "channel"
            ]
            self.assertIn(f"Rust {toolchain}", instructions)
            self.assertIn("build-essential pkg-config", instructions)
            self.assertIn("libxkbcommon-dev", instructions)
            self.assertIn("libudev-dev", instructions)
            self.assertIn("xcode-select --install", instructions)
            self.assertIn(
                "cargo build --profile dist --locked --offline --features xtb --bin reshiki",
                instructions,
            )
            command = next(
                line
                for line in instructions.replace("\\\n", "").splitlines()
                if line.startswith("cargo build --offline")
            )
            arguments = shlex.split(command)
            self.assertNotIn("--locked", arguments)
            self.assertIn("--offline", arguments)
            self.assertEqual(
                arguments[arguments.index("--config") :], patch_arguments(root / "axtb-source")
            )
            self.assertIn("codesign --force --sign - target/dist/reshiki", instructions)
            self.assertIn("bundle_metadata", instructions)
            self.assertIn("licenses/axtb/", instructions)

    def test_modify_driver_version_changes_only_the_constant_line(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            fork_fixture(source)
            response = source / "crates/driver/src/response.rs"
            original = response.read_text()
            response.write_text("// Before\n" + original + "// After\n", encoding="utf-8")
            self.assertEqual(modify_driver_version(source, "0.2.0"), "0.2.0+modified")
            self.assertEqual(
                response.read_text(),
                '// Before\npub const VERSION: &str = "0.2.0+modified";\n// After\n',
            )

    def test_check_builds_from_empty_cargo_home_with_documented_patches(self):
        with tempfile.TemporaryDirectory() as temporary:
            temporary = Path(temporary)
            root = temporary / "reshiki-fixture-source"
            fork_fixture(root / "axtb-source")
            write_manifest(root, {"axtb": {"rev": "a" * 40, "version": "0.2.0"}})
            archive = temporary / "fixture.tar.gz"
            write_archive(root, archive)

            def build(command, *, cwd, env):
                self.assertIn(" ", str(cwd))
                self.assertEqual(
                    command[:7],
                    ["cargo", "build", "--offline", "--features", "xtb", "--bin", "reshiki"],
                )
                self.assertEqual(command[7:], patch_arguments(cwd / "axtb-source"))
                self.assertNotIn("--locked", command)
                self.assertEqual(env["CARGO_NET_OFFLINE"], "true")
                self.assertEqual(list(Path(env["CARGO_HOME"]).iterdir()), [])
                self.assertEqual(Path(env["CARGO_TARGET_DIR"]), cwd / "target")
                response = (cwd / "axtb-source/crates/driver/src/response.rs").read_text()
                self.assertIn('"0.2.0+modified"', response)

            def verify(binary, metadata):
                self.assertEqual(binary.parent.name, "debug")
                self.assertEqual(
                    metadata["xtb"],
                    {"included": True, "rev": "a" * 40, "version": "0.2.0+modified"},
                )
                print("axtb 0.2.0+modified water energy verified")

            summary = io.StringIO()
            with (
                patch("xtb_source_archive.run", side_effect=build) as builder,
                patch("build_release.verify_xtb_worker", side_effect=verify) as verifier,
                contextlib.redirect_stdout(summary),
            ):
                self.assertGreaterEqual(check(archive), 0)
            builder.assert_called_once()
            verifier.assert_called_once()
            self.assertEqual(len(summary.getvalue().splitlines()), 1)
            self.assertIn("0.2.0+modified", summary.getvalue())
            self.assertIn("water energy verified", summary.getvalue())

    def test_check_refuses_bad_manifest_before_modification_or_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            temporary = Path(temporary)
            root = temporary / "reshiki-fixture-source"
            root.mkdir()
            (root / "file").write_bytes(b"original")
            write_manifest(root, {})
            (root / "file").write_bytes(b"tampered")
            archive = temporary / "fixture.tar.gz"
            write_archive(root, archive)
            with (
                patch("xtb_source_archive.run") as builder,
                patch("xtb_source_archive.modify_driver_version") as modifier,
                self.assertRaisesRegex(ValueError, "Source manifest SHA-256 mismatch"),
            ):
                check(archive)
            modifier.assert_not_called()
            builder.assert_not_called()
            self.assertEqual(
                json.loads((root / MANIFEST).read_text())["files"]["file"],
                hashlib.sha256(b"original").hexdigest(),
            )


if __name__ == "__main__":
    unittest.main()
