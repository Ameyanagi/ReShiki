"""Regression checks for release identity, archive naming, and secret handling."""

import contextlib
import hashlib
import io
import json
import os
import plistlib
import subprocess
import sys
import tempfile
import tomllib
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from build_release import (
    archive,
    check_tag,
    mac_bundle,
    main,
    notices,
    numeric_version,
    release_platform,
    verify_binary,
    version,
)
from sign_macos import is_macho, private_run


class ReleaseTests(unittest.TestCase):
    def test_nightly_native_versions_preserve_full_identity_outside_numeric_fields(self):
        nightly = "0.9.1-nightly.20260929.36501221724.1"
        self.assertEqual(numeric_version(nightly), "0.9.1")
        self.assertEqual(numeric_version("1.2.3"), "1.2.3")
        for invalid in ("0.9", "65536.1.1", "1.2.bad", "../0.9.1"):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                numeric_version(invalid)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "target/release/reshiki"
            binary.parent.mkdir(parents=True)
            binary.touch()

            def run(command, **_kwargs):
                if command[0] == "swiftc":
                    Path(command[command.index("-o") + 1]).touch()

            app = root / "ReShiki.app"
            with (
                patch("build_release.version", return_value=nightly),
                patch("build_release.target_directory", return_value=root / "target"),
                patch("build_release.run", side_effect=run),
                patch("build_release.notices"),
            ):
                mac_bundle(app, "release")
            self.assertFalse((app / "Contents/Helpers/ReShiki Print.app").exists())
            for info_path in (app / "Contents/Info.plist",):
                info = plistlib.loads(info_path.read_bytes())
                self.assertEqual(info["CFBundleVersion"], "0.9.1")
                self.assertEqual(info["CFBundleShortVersionString"], "0.9.1")
                self.assertEqual(info["ReShikiPackageVersion"], nightly)

    def test_nightly_windows_setup_uses_numeric_resource_and_full_display_version(self):
        from installers import windows_installer

        nightly = "0.9.1-nightly.20260929.36501221724.1"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            folder = root / f"reshiki-{nightly}-windows-arm64"
            folder.mkdir()
            (folder / "build.json").write_text(
                json.dumps(dict(version=nightly, platform="windows", architecture="arm64"))
            )
            (folder / "reshiki.exe").write_bytes(self.pe_image(0xAA64))

            def compile_setup(command):
                self.assertIn(f"/DAppVersion={nightly}", command)
                self.assertIn("/DAppNumericVersion=0.9.1", command)
                (root / f"{folder.name}-setup.exe").write_bytes(b"installer fixture")

            with (
                patch("installers.inno_compiler", return_value=Path("ISCC.exe")),
                patch("installers.run", side_effect=compile_setup),
                patch("installers.verify_windows_installer") as verify,
                patch("installers.verify_windows_upgrade_with_running_agent") as upgrade,
            ):
                result = windows_installer(folder, root)
            verify.assert_called_once_with(result, folder)
            upgrade.assert_called_once_with(result, folder)
            self.assertTrue(Path(str(result) + ".sha256").is_file())
            script = (
                Path(__file__).resolve().parents[1] / "packaging/windows/reshiki.iss"
            ).read_text()
            self.assertIn("VersionInfoVersion={#AppNumericVersion}", script)
            self.assertIn("VersionInfoTextVersion={#AppVersion}", script)

    def test_nightly_signing_requires_stamped_version_and_matching_source(self):
        from prepare_nightly import stamp
        from sign_release import verify_source

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = Path(__file__).resolve().parents[1]
            for filename in ("Cargo.toml", "Cargo.lock"):
                (root / filename).write_bytes((source / filename).read_bytes())
            nightly = numeric_version(version()) + "-nightly.20260929.36501221724.1"
            metadata = dict(version=nightly, commit="qualified-commit")
            with (
                patch("build_release.ROOT", root),
                patch.dict(os.environ, {"GITHUB_SHA": "qualified-commit"}),
            ):
                with self.assertRaisesRegex(ValueError, "source does not match"):
                    verify_source(metadata)
                stamp(root, version=nightly)
                verify_source(metadata)
                metadata["commit"] = "another-commit"
                with self.assertRaisesRegex(ValueError, "source does not match"):
                    verify_source(metadata)

    def test_signed_checksum_preserves_provenance_and_follows_archive_verification(self):
        import sign_release

        for failure in (None, "checksum", "verification"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                unsigned = root / "unsigned"
                unsigned.mkdir()
                source = unsigned / "reshiki-1.2.3-macos-arm64.zip"
                source.write_bytes(b"qualified unsigned archive")
                unsigned_digest = hashlib.sha256(source.read_bytes()).hexdigest()
                Path(str(source) + ".sha256").write_text(
                    f"{unsigned_digest}  {source.name}\n", encoding="ascii"
                )
                if failure == "checksum":
                    source.write_bytes(b"changed unsigned archive")
                output = root / "dist/releases" / source.name
                signed_bytes = b"signed archive with notarization fixture"
                signed_digest = hashlib.sha256(signed_bytes).hexdigest()
                expected_checksum = f"{signed_digest}  {output.name}\n".encode("ascii")
                metadata = dict(version="1.2.3", commit="qualified-commit", signed=False)
                events = []

                def extract(command):
                    self.assertEqual(command[:4], ["ditto", "-x", "-k", source])
                    events.append("extract")
                    folder = command[4] / source.stem
                    (folder / "ReShiki.app").mkdir(parents=True)
                    (folder / "build.json").write_text(json.dumps(metadata))
                    (folder / "README.txt").write_text("This build has no publisher signature.")

                def archive(folder, destination):
                    events.append("archive")
                    self.assertEqual(destination, output.with_suffix(""))
                    self.assertEqual(
                        json.loads((folder / "build.json").read_text()),
                        dict(
                            metadata, signed=True, notarized=True, unsigned_sha256=unsigned_digest
                        ),
                    )
                    self.assertEqual(
                        (folder / "README.txt").read_text(),
                        "macOS application signed with Developer ID and notarized by Apple.",
                    )
                    output.parent.mkdir(parents=True)
                    output.write_bytes(signed_bytes)
                    return output

                def verify(archive_path, *, signed):
                    events.append("verify")
                    self.assertEqual(archive_path, output)
                    self.assertTrue(signed)
                    self.assertFalse(Path(str(output) + ".sha256").exists())
                    if failure == "verification":
                        raise ValueError("fixture archive verification failed")

                def disk_image(folder, destination, *, signed):
                    events.append("disk image")
                    self.assertEqual(folder.name, source.stem)
                    self.assertEqual(destination, output.parent)
                    self.assertTrue(signed)
                    self.assertEqual(Path(str(output) + ".sha256").read_bytes(), expected_checksum)

                with (
                    patch.object(sign_release, "ROOT", root),
                    patch.object(sign_release, "version", return_value="1.2.3"),
                    patch.dict(os.environ, {"GITHUB_SHA": "qualified-commit"}),
                    patch.object(sign_release, "run", side_effect=extract),
                    patch.object(
                        sign_release,
                        "sign_and_notarize",
                        side_effect=lambda _: events.append("sign"),
                    ),
                    patch.object(sign_release, "archive", side_effect=archive),
                    patch.object(sign_release, "verify_archive", side_effect=verify),
                    patch.object(sign_release, "mac_disk_image", side_effect=disk_image),
                ):
                    if failure:
                        message = (
                            "Archive checksum mismatch"
                            if failure == "checksum"
                            else "fixture archive verification failed"
                        )
                        with self.assertRaisesRegex(ValueError, message):
                            sign_release.main()
                        self.assertFalse(Path(str(output) + ".sha256").exists())
                    else:
                        sign_release.main()
                        self.assertEqual(
                            Path(str(output) + ".sha256").read_bytes(), expected_checksum
                        )
                self.assertEqual(
                    events,
                    []
                    if failure == "checksum"
                    else ["extract", "sign", "archive", "verify"]
                    + ([] if failure == "verification" else ["disk image"]),
                )

    def test_release_includes_adapted_source_licenses(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "Licenses"
            with (
                patch("build_release.run") as run,
                patch("geometry_source.verify") as geometry,
            ):
                run.return_value = subprocess.CompletedProcess([], 0, stdout='{"packages": []}')
                notices(destination)
            geometry.assert_called_once_with(
                Path(__file__).resolve().parents[1], cargo_metadata={"packages": []}
            )
            self.assertIn(
                "BSD 3-Clause License",
                (destination / "THIRD-PARTY-NOTICES.txt").read_text(),
            )
            self.assertIn(
                "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
                (destination / "THIRD-PARTY-NOTICES.txt").read_text(),
            )
            self.assertIn("MIT OR Apache-2.0", (destination / "LICENSE").read_text())
            self.assertIn(
                "Copyright (c) 2026 Ameyanagi and ReShiki contributors",
                (destination / "LICENSE-MIT").read_text(),
            )
            self.assertIn("Version 2.0, January 2004", (destination / "LICENSE-APACHE").read_text())

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
            with (
                patch("build_release.ROOT", root),
                patch("build_release.version", return_value="1.2.3"),
                patch("build_release.platform.system", return_value="Windows"),
                patch("build_release.platform.machine", return_value="AMD64"),
                patch("build_release.target_directory", return_value=root / "target"),
                patch("build_release.notices"),
                patch(
                    "build_inchi_helper.dependency",
                    return_value={"name": "cosmolkit-inchi", "version": "0.3.0"},
                ),
                patch(
                    "geometry_source.verify",
                    return_value={
                        "version": "2026.03.6",
                        "implementation": "Rust",
                        "runtime": "self-process-rust-core",
                        "runtime_python": False,
                        "backend": {"name": "cosmolkit-core", "version": "0.3.0"},
                    },
                ),
                patch("build_release.verify_archive"),
                patch("build_release.run") as run,
                patch("sys.argv", ["build_release.py", "--target", target]),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                main()
            run.assert_called_once_with(
                ["cargo", "build", "--release", "--locked", "--bin", "reshiki", "--target", target],
                cwd=root,
            )
            self.assertTrue((root / "build/release-bundles/reshiki-1.2.3-windows-arm64").is_dir())
            self.assertFalse((root / "target/release-bundles").exists())
            package = root / "dist/releases/reshiki-1.2.3-windows-arm64.zip"
            checksum = Path(str(package) + ".sha256").read_bytes()
            digest = hashlib.sha256(package.read_bytes()).hexdigest()
            # The Linux publisher must be able to verify Windows-generated manifests.
            self.assertEqual(checksum, f"{digest}  {package.name}\n".encode("ascii"))
            with zipfile.ZipFile(package) as stream:
                metadata = json.loads(stream.read("reshiki-1.2.3-windows-arm64/build.json"))
                self.assertEqual(metadata["architecture"], "arm64")
                self.assertEqual(metadata["rust_target"], target)
                self.assertNotIn("chemistry_architecture", metadata)
                self.assertFalse(any("chemistry/" in name for name in stream.namelist()))
                self.assertFalse(
                    any(name.endswith((".py", "uv.lock")) for name in stream.namelist())
                )
                self.assertEqual(metadata["inchi"]["version"], "1.07.5")
                self.assertEqual(metadata["inchi"]["runtime"], "self-process")
                self.assertEqual(metadata["inchi"]["dependency"]["name"], "cosmolkit-inchi")
                self.assertEqual(metadata["geometry"]["backend"]["name"], "cosmolkit-core")
                self.assertEqual(metadata["geometry"]["implementation"], "Rust")
                self.assertEqual(metadata["geometry"]["runtime"], "self-process-rust-core")
                self.assertEqual(metadata["geometry"]["linkage"], "static")
                self.assertFalse(metadata["geometry"]["runtime_python"])
                self.assertEqual(
                    [name for name in stream.namelist() if name.endswith(".exe")],
                    ["reshiki-1.2.3-windows-arm64/reshiki.exe"],
                )
                self.assertFalse(metadata["signed"])
                self.assertIn(
                    b"Windows 11 on ARM is required",
                    stream.read("reshiki-1.2.3-windows-arm64/README.txt"),
                )

    def test_packages_build_only_the_single_application_executable(self):
        root = Path(__file__).resolve().parents[1]
        manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
        # The default binary is src/main.rs; the InChI helper is a development tool.
        self.assertEqual(manifest["package"]["default-run"], "reshiki")
        self.assertTrue((root / "src/main.rs").is_file())
        # Cargo would also build every src/bin target automatically.
        self.assertFalse((root / "src/bin").exists())
        self.assertEqual([entry["name"] for entry in manifest["bin"]], ["reshiki-inchi-helper"])
        script = (root / "scripts/build_release.py").read_text(encoding="utf-8")
        self.assertEqual(script.count('"--bin"'), 1)
        self.assertIn('"--bin", "reshiki", "--target"', script)

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

    def test_release_targets_include_intel_mac_and_reject_unknown_architectures(self):
        self.assertEqual(release_platform("aarch64-unknown-linux-gnu"), ("linux", "arm64"))
        self.assertEqual(release_platform("x86_64-apple-darwin"), ("macos", "x64"))
        with self.assertRaisesRegex(ValueError, "Unsupported release target"):
            release_platform("riscv64gc-unknown-linux-gnu")

    def test_intel_macos_app_and_helper_headers_reject_arm64(self):
        from build_inchi_helper import verify_executable

        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "native"
            binary.write_bytes(b"\xcf\xfa\xed\xfe" + (0x01000007).to_bytes(4, "little"))
            verify_binary(binary, "macos", "x64")
            self.assertEqual(
                verify_executable(binary, "x86_64-apple-darwin"), "x86_64-apple-darwin"
            )
            with self.assertRaisesRegex(ValueError, "architecture mismatch"):
                verify_executable(binary, "aarch64-apple-darwin")
            with self.assertRaisesRegex(ValueError, "Expected macos arm64"):
                verify_binary(binary, "macos", "arm64")

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

    def test_windows_archive_accepts_reproducible_source_timestamps(self):
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
