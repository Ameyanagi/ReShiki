"""Regression checks for release identity, archive naming, and secret handling."""

import contextlib
import hashlib
import io
import json
import os
import plistlib
import signal
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import tomllib
import unittest
import zipfile
from contextlib import redirect_stdout
from pathlib import Path
from types import SimpleNamespace
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
from check_agent_dependencies import AXTB_LICENSE, AXTB_SOURCE, axtb_locked_source
from sign_macos import is_macho, private_run

AXTB_REV = "a" * 40
AXTB_DRIVER = dict(name="axtb-driver", version="0.2.0", source=axtb_locked_source(AXTB_REV))


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
                patch("installers.normalize_windows_installer_metadata") as normalize,
                patch("installers.verify_windows_installer") as verify,
                patch("installers.verify_windows_upgrade_with_running_agent") as upgrade,
            ):
                result = windows_installer(folder, root)
            normalize.assert_called_once_with(result, nightly)
            verify.assert_called_once_with(result, folder)
            upgrade.assert_called_once_with(result, folder)
            self.assertTrue(Path(str(result) + ".sha256").is_file())
            script = (
                Path(__file__).resolve().parents[1] / "packaging/windows/reshiki.iss"
            ).read_text()
            self.assertEqual(
                [
                    line
                    for line in script.splitlines()
                    if line.startswith("VersionInfoProductName=")
                ],
                ["VersionInfoProductName=ReShiki"],
            )
            self.assertIn("VersionInfoVersion={#AppNumericVersion}", script)
            self.assertIn("VersionInfoTextVersion={#AppVersion}", script)
            self.assertIn("VersionInfoProductTextVersion={#AppVersion}", script)

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
                patch("build_release.verify_agent_dependencies") as dependencies,
                patch("geometry_source.verify") as geometry,
            ):
                run.return_value = subprocess.CompletedProcess([], 0, stdout='{"packages": []}')
                notices(destination)
            dependencies.assert_called_once_with(
                Path(__file__).resolve().parents[1], {"packages": []}
            )
            geometry.assert_called_once_with(
                Path(__file__).resolve().parents[1], cargo_metadata={"packages": []}
            )
            self.assertIn(
                "BSD 3-Clause License",
                (destination / "THIRD-PARTY-NOTICES.txt").read_text(encoding="utf-8"),
            )
            self.assertIn(
                "0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985",
                (destination / "THIRD-PARTY-NOTICES.txt").read_text(encoding="utf-8"),
            )
            self.assertIn("MIT OR Apache-2.0", (destination / "LICENSE").read_text())
            self.assertIn(
                "Copyright (c) 2026 Ameyanagi and ReShiki contributors",
                (destination / "LICENSE-MIT").read_text(),
            )
            self.assertIn("Version 2.0, January 2004", (destination / "LICENSE-APACHE").read_text())

    def test_xtb_notices_use_feature_metadata_after_default_audits(self):
        root = Path(__file__).resolve().parents[1]
        default = {"packages": []}
        driver = {**AXTB_DRIVER, "id": "xtb"}
        xtb = {
            "packages": [
                {"id": "app", "name": "reshiki"},
                driver,
                {"id": "python", "name": "reshiki-python"},
                {"id": "pyo3", "name": "pyo3"},
            ],
            "resolve": {
                "root": "app",
                "nodes": [
                    {"id": "app", "deps": [{"pkg": "xtb", "dep_kinds": [{"kind": None}]}]},
                    {"id": "xtb", "deps": []},
                    {"id": "python", "deps": [{"pkg": "pyo3", "dep_kinds": [{"kind": None}]}]},
                    {"id": "pyo3", "deps": []},
                ],
            },
        }
        target = "aarch64-unknown-linux-gnu"
        destination = Path("Licenses")
        with (
            patch(
                "build_release.run",
                side_effect=[
                    subprocess.CompletedProcess([], 0, stdout=json.dumps(metadata))
                    for metadata in (default, xtb)
                ],
            ) as run,
            patch("build_release.verify_agent_dependencies") as dependencies,
            patch("geometry_source.verify") as geometry,
            patch("build_release.write_notices") as write,
        ):
            notices(destination, ("xtb",), target=target)
        command = ["cargo", "metadata", "--format-version", "1", "--locked"]
        self.assertEqual(
            [call.args[0] for call in run.call_args_list],
            [command, command + ["--features", "xtb", "--filter-platform", target]],
        )
        dependencies.assert_called_once_with(root, default)
        geometry.assert_called_once_with(root, cargo_metadata=default)
        write.assert_called_once_with(root, destination, {**xtb, "packages": [driver]})

    @staticmethod
    def pe_image(machine):
        header = bytearray(64)
        header[:2] = b"MZ"
        header[60:64] = (64).to_bytes(4, "little")
        return header + b"PE\0\0" + machine.to_bytes(2, "little")

    @staticmethod
    def package(root, target, system):
        """Run build_release.main() for target with cargo and the checks mocked out."""
        with (
            patch("build_release.ROOT", root),
            patch("build_release.version", return_value="1.2.3"),
            patch("build_release.platform.system", return_value=system),
            patch("build_release.platform.machine", return_value="AMD64"),
            patch("build_release.target_directory", return_value=root / "target"),
            patch("build_release.notices"),
            patch("build_release.cargo_metadata", return_value={"packages": [AXTB_DRIVER]}),
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
            patch.dict(os.environ, RELEASE_TAG="v1.2.3"),
            contextlib.redirect_stdout(io.StringIO()),
        ):
            main()
        return run

    def test_linux_package_ships_a_stripped_binary_and_keeps_its_symbols(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = "x86_64-unknown-linux-gnu"
            build = root / "target" / target / "dist"
            build.mkdir(parents=True)
            elf = bytearray(64)
            elf[:6] = b"\x7fELF\x02\x01"
            elf[18:20] = (62).to_bytes(2, "little")
            (build / "reshiki").write_bytes(elf)
            (root / "Cargo.lock").write_text(
                "[[package]]\n"
                + "".join(f'{key} = "{value}"\n' for key, value in AXTB_DRIVER.items()),
                encoding="utf-8",
            )
            run = self.package(root, target, "Linux")
            packaged = root / "build/release-bundles/reshiki-1.2.3-linux-x64/reshiki"
            symbols = build / "reshiki.debug"
            self.assertEqual(run.call_args_list[0].args[0][-2:], ["--features", "xtb"])
            self.assertEqual(
                [call.args[0] for call in run.call_args_list[1:]],
                [
                    ["objcopy", "--only-keep-debug", build / "reshiki", symbols],
                    ["objcopy", "--strip-all", f"--add-gnu-debuglink={symbols}", packaged],
                ],
            )
            name = "reshiki-1.2.3-linux-x64"
            with tarfile.open(root / "dist/releases" / f"{name}.tar.gz") as stream:
                metadata = json.loads(stream.extractfile(f"{name}/build.json").read())
                readme = stream.extractfile(f"{name}/README.txt").read().decode()
            self.assertEqual(
                metadata["xtb"],
                dict(
                    included=True,
                    engine="axtb",
                    version=AXTB_DRIVER["version"],
                    license=AXTB_LICENSE,
                    source=AXTB_SOURCE,
                    rev=AXTB_REV,
                    source_archive="reshiki-1.2.3-source.tar.gz",
                ),
            )
            self.assertIn("reshiki-1.2.3-source.tar.gz", readme)
            self.assertIn("https://github.com/Ameyanagi/ReShiki/releases/tag/v1.2.3", readme)

    def test_windows_arm_package_uses_rust_target_under_x64_python(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = "aarch64-pc-windows-msvc"
            binary = root / "target" / target / "dist/reshiki.exe"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(self.pe_image(0xAA64))
            (root / "Cargo.lock").write_text(
                "[[package]]\n"
                + "".join(f'{key} = "{value}"\n' for key, value in AXTB_DRIVER.items()),
                encoding="utf-8",
            )
            run = self.package(root, target, "Windows")
            run.assert_called_once_with(
                [
                    "cargo",
                    "build",
                    "--profile",
                    "dist",
                    "--locked",
                    "--bin",
                    "reshiki",
                    "--target",
                    target,
                    "--features",
                    "xtb",
                ],
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
                self.assertTrue(metadata["xtb"]["included"])
                self.assertEqual(metadata["xtb"]["rev"], AXTB_REV)
                self.assertEqual(metadata["xtb"]["license"], AXTB_LICENSE)
                self.assertIn("axtb", stream.read(f"{package.stem}/README.txt").decode())
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
        self.assertRegex(script, r'"--bin",\s*"reshiki",\s*"--target"')

    def test_release_profile_strips_symbols_into_a_separate_file(self):
        root = Path(__file__).resolve().parents[1]
        manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
        profile = manifest["profile"]["release"]
        self.assertEqual(profile["strip"], "symbols")
        self.assertEqual(profile["split-debuginfo"], "packed")
        # The app recovers from panics with catch_unwind, so panics must unwind.
        self.assertNotIn("panic", profile)

    def test_published_builds_use_one_optimized_unit_and_the_release_symbol_settings(self):
        root = Path(__file__).resolve().parents[1]
        manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
        self.assertEqual(
            manifest["profile"]["dist"], {"inherits": "release", "lto": "fat", "codegen-units": 1}
        )
        # Tests and local runs keep the faster release profile.
        self.assertNotIn("lto", manifest["profile"]["release"])
        workflow = (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.assertNotIn("/release/reshiki", workflow)
        self.assertNotIn("CARGO_PROFILE_RELEASE_", workflow)
        # Only macOS strips in Cargo (into reshiki.dSYM). Windows needs an
        # unstripped link for reshiki.pdb; Linux keeps its symbols in the
        # binary for build_release.py to split into reshiki.debug.
        for setting in (
            "CARGO_PROFILE_DIST_STRIP: "
            "${{ startsWith(matrix.artifact, 'macos') && 'symbols' || 'none' }}",
            "CARGO_PROFILE_DIST_SPLIT_DEBUGINFO: "
            "${{ startsWith(matrix.artifact, 'linux') && 'off' || 'packed' }}",
            # A fat-LTO link needs more memory than the macOS, Linux and
            # Windows x64 runners have. Linux and Windows x64 build without LTO
            # across crates, with the release profile's 16 codegen units;
            # "false" is a non-empty string, so the expression yields it. Only
            # Windows arm64 keeps fat LTO.
            "CARGO_PROFILE_DIST_LTO: ${{ startsWith(matrix.artifact, 'macos') && 'thin' || "
            "matrix.artifact == 'windows-arm64' && 'fat' || 'false' }}",
            "CARGO_PROFILE_DIST_CODEGEN_UNITS: "
            "${{ startsWith(matrix.artifact, 'linux') && '16' || "
            "matrix.artifact == 'windows-x64' && '16' || '1' }}",
        ):
            self.assertEqual(workflow.count(setting), 1, setting)
        for path in (
            "target/${{ matrix.target }}/dist/reshiki.dSYM",
            "target/${{ matrix.target }}/dist/reshiki.pdb",
            "target/${{ matrix.target }}/dist/reshiki.debug",
            "RESHIKI_TEST_PACKAGED_APP: ${{ github.workspace }}/target/${{ matrix.target }}"
            "/dist/reshiki${{ runner.os == 'Windows' && '.exe' || '' }}",
        ):
            self.assertIn(path, workflow)
        script = (root / "scripts/build_release.py").read_text(encoding="utf-8")
        self.assertRegex(script, r'"cargo",\s*"build",\s*"--profile",\s*"dist",\s*"--locked"')
        self.assertIn('target_directory() / target / "dist"', script)

    def test_each_npm_package_ships_only_its_own_webassembly(self):
        root = Path(__file__).resolve().parents[1]
        exports = {
            name: json.loads((root / "packages" / name / "package.json").read_text("utf-8"))[
                "exports"
            ]
            for name in ("editor", "headless")
        }
        self.assertEqual(
            exports["headless"],
            {
                ".": {"types": "./src/index.d.ts", "default": "./src/index.js"},
                "./reshiki_web_bg.wasm": "./dist/wasm/reshiki_web_bg.wasm",
            },
        )
        self.assertNotIn(".", exports["editor"])
        self.assertNotIn("dist/wasm", json.dumps(exports["editor"]))
        script = (root / "scripts/build_wasm.sh").read_text(encoding="utf-8")
        self.assertIn(
            "reshiki_web:headless/dist/wasm reshiki:editor/dist/editor \\\n"
            "  reshiki_web_worker:editor/dist/worker",
            script,
        )
        workflow = (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.assertIn("for package in headless editor; do", workflow)

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
        for package_version, tag in (
            ("0.17.1", "v0.17.1"),
            ("0.17.1-nightly.1", "v0.17.1-nightly.1"),
            ("0.17.1-nightly.1", "nightly-0.17.1-nightly.1"),
            (
                "0.9.1-nightly.20260929.36501221724.1",
                "nightly-0.9.1-nightly.20260929.36501221724.1",
            ),
        ):
            with (
                self.subTest(tag=tag),
                patch("build_release.version", return_value=package_version),
            ):
                check_tag(tag)
                for invalid in ("v0.0.0-unrelated", "nightly-0.0.0-unrelated", package_version):
                    with (
                        self.subTest(invalid=invalid),
                        self.assertRaisesRegex(ValueError, "must match"),
                    ):
                        check_tag(invalid)

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


class XtbReleaseWorkerTests(unittest.TestCase):
    WORKER = r"""
import json
import struct
import subprocess
import sys
import time
from pathlib import Path

config = json.loads(sys.argv[1])
if config["mode"] == "no_reply":
    time.sleep(30)
if config["mode"] == "inherited_pipe":
    child = subprocess.Popen(
        [sys.executable, "-c", "import time; time.sleep(30)"],
        start_new_session=config["detached"],
    )
    Path(config["child_pid"]).write_text(str(child.pid))
header = struct.Struct("<8sHHI")
while data := sys.stdin.buffer.read(header.size):
    _, _, _, length = header.unpack(data)
    request = json.loads(sys.stdin.buffer.read(length))
    with open(config["requests"], "a") as stream:
        stream.write(json.dumps(request) + "\n")
    response = json.dumps({"id": request["id"], "ok": config["replies"][request["op"]]}).encode()
    sys.stdout.buffer.write(header.pack(b"RSHXTBWK", 2, 0, len(response)) + response)
    sys.stdout.buffer.flush()
    if request["op"] == "close":
        break
"""

    @staticmethod
    def replies():
        from build_release import XTB_WATER_ENERGY

        return {
            "hello": {
                "protocol": 2,
                "engine": {"name": "axtb", "version": "0.2.0", "rev": AXTB_REV},
                "methods": ["gfn2"],
            },
            "open": {"conventions": "axtb"},
            "compute": {
                "conventions": "axtb",
                "energy": XTB_WATER_ENERGY,
                "gradient": [[0.0, 0.0, 0.0] for _ in range(3)],
                "homo_lumo_gap_ev": 14.0,
                "near_degenerate": False,
            },
            "close": {},
        }

    def verify_fixture(
        self, *, replies=None, included=True, mode="normal", detached=False, portable=False
    ):
        import build_release

        popen = subprocess.Popen
        killpg = getattr(os, "killpg", None)
        processes = []
        threads = set(threading.enumerate())
        self.group_kills = []
        output = io.StringIO()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            worker = root / "worker.py"
            worker.write_text(self.WORKER, encoding="utf-8")
            config = dict(
                replies=self.replies() if replies is None else replies,
                mode=mode,
                detached=detached,
                requests=str(root / "requests.jsonl"),
                child_pid=str(root / "child.pid"),
            )

            def launch(command, **kwargs):
                self.assertEqual(command, [Path("fixture-reshiki").resolve(), "--xtb-worker"])
                self.assertEqual(kwargs["start_new_session"], os.name == "posix" and not portable)
                self.worker_environment = kwargs["env"].copy()
                process = popen([sys.executable, worker, json.dumps(config)], **kwargs)
                processes.append(process)
                return process

            def kill_group(pid, sig):
                self.group_kills.append((pid, sig, processes[0].poll()))
                return killpg(pid, sig)

            try:
                with contextlib.ExitStack() as stack:
                    stack.enter_context(patch("build_release.subprocess.Popen", side_effect=launch))
                    stack.enter_context(
                        patch(
                            "build_release.XTB_VERIFY_TIMEOUT_SECONDS", 5 if mode == "normal" else 1
                        )
                    )
                    if portable:
                        stack.enter_context(
                            patch(
                                "build_release.os", SimpleNamespace(name="nt", environ=os.environ)
                            )
                        )
                    elif killpg is not None:
                        stack.enter_context(
                            patch("build_release.os.killpg", side_effect=kill_group)
                        )
                    stack.enter_context(redirect_stdout(output))
                    started = time.monotonic()
                    try:
                        build_release.verify_xtb_worker(
                            Path("fixture-reshiki"),
                            {"xtb": dict(included=included, version="0.2.0", rev=AXTB_REV)},
                        )
                    finally:
                        self.verification_elapsed = time.monotonic() - started
                requests = [
                    json.loads(line) for line in (root / "requests.jsonl").read_text().splitlines()
                ]
                return requests, output.getvalue()
            finally:
                if (root / "child.pid").exists():
                    try:
                        os.kill(int((root / "child.pid").read_text()), signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                for process in processes:
                    if process.poll() is None:
                        process.kill()
                    process.wait(timeout=2)
                for thread in set(threading.enumerate()) - threads:
                    if thread.name == "ReShiki xTB check":
                        thread.join(timeout=2)

    def test_water_uses_axtb_conventions_and_checks_gap_diagnostics(self):
        requests, summary = self.verify_fixture()
        self.assertEqual(
            [request["op"] for request in requests], ["hello", "open", "compute", "close"]
        )
        self.assertEqual(requests[1]["conventions"], "axtb")
        self.assertIn("xTB worker axtb 0.2.0: water -5.070222289327834 Eh, gradient OK.", summary)

    def test_stub_reports_protocol_without_opening_a_session(self):
        replies = self.replies()
        replies["hello"].update(engine=None, methods=[])
        requests, summary = self.verify_fixture(replies=replies, included=False)
        self.assertEqual([request["op"] for request in requests], ["hello"])
        self.assertEqual(summary, "xTB worker protocol 2 verified with no engine.\n")

    def test_worker_ignores_external_lapack_overrides_and_preserves_other_environment(self):
        environment = {
            "AXTB_LAPACK": "/external/lapack",
            "AXTB_LAPACK_BLAS": "/external/blas",
            "RESHIKI_TEST_XTB_ENV": "preserved",
        }
        with patch.dict(os.environ, environment):
            self.verify_fixture()
            self.assertNotIn("AXTB_LAPACK", self.worker_environment)
            self.assertNotIn("AXTB_LAPACK_BLAS", self.worker_environment)
            self.assertEqual(self.worker_environment["RESHIKI_TEST_XTB_ENV"], "preserved")
            for name, value in environment.items():
                self.assertEqual(os.environ[name], value)

    def test_water_rejects_wrong_conventions_and_gap_diagnostics(self):
        cases = [
            ("open", "conventions", "xtb-6.8", "conventions"),
            ("compute", "conventions", "xtb-6.8", "conventions"),
            ("compute", "conventions", None, "conventions"),
            ("compute", "near_degenerate", True, "gap diagnostics"),
            ("compute", "near_degenerate", None, "gap diagnostics"),
        ] + [
            ("compute", "homo_lumo_gap_ev", gap, "gap diagnostics")
            for gap in (None, float("nan"), float("inf"), -1.0, 0.05, True)
        ]
        for operation, field, value, message in cases:
            with self.subTest(operation=operation, field=field, value=value):
                replies = self.replies()
                replies[operation][field] = value
                with self.assertRaisesRegex(ValueError, message):
                    self.verify_fixture(replies=replies)

    def test_timeout_kills_worker_process_group(self):
        with self.assertRaisesRegex(ValueError, "did not complete within 1 seconds"):
            self.verify_fixture(mode="no_reply")
        self.assertLess(self.verification_elapsed, 3)
        if os.name == "posix":
            self.assertTrue(self.group_kills)
            self.assertTrue(
                all(sig == signal.SIGKILL and status is None for _, sig, status in self.group_kills)
            )

    @unittest.skipUnless(os.name == "posix", "POSIX process-group reaping regression")
    def test_timeout_keeps_reader_from_reaping_during_group_signal(self):
        import build_release

        body = json.dumps(dict(id=1, ok=dict(protocol=2, engine=None, methods=[]))).encode()
        process = SimpleNamespace(
            pid=12345,
            returncode=None,
            stdin=io.BytesIO(),
            stdout=io.BytesIO(build_release.XTB_HEADER.pack(b"RSHXTBWK", 2, 0, len(body)) + body),
        )
        exit_requested = threading.Event()
        reader_waiting = threading.Event()
        reader_reaped = threading.Event()

        def poll():
            reader = threading.current_thread().name == "ReShiki xTB check"
            if reader:
                reader_waiting.set()
            if exit_requested.is_set():
                process.returncode = 0
                if reader:
                    reader_reaped.set()
            return process.returncode

        def wait():
            reader_waiting.set()
            if not exit_requested.wait(timeout=2):
                raise AssertionError("A blocking reader wait prevented timeout cleanup")
            return poll()

        def kill_group(pid, sig):
            self.assertEqual((pid, sig), (process.pid, signal.SIGKILL))
            self.assertTrue(reader_waiting.is_set())
            # Simulate exit during signaling; the reader must not reap it yet.
            exit_requested.set()
            self.assertFalse(reader_reaped.wait(timeout=0.1))

        process.poll = poll
        process.wait = wait
        threads = set(threading.enumerate())
        started = time.monotonic()
        try:
            with (
                patch("build_release.subprocess.Popen", return_value=process),
                patch("build_release.os.killpg", side_effect=kill_group) as signal_group,
                patch("build_release.XTB_VERIFY_TIMEOUT_SECONDS", 0.2),
                self.assertRaisesRegex(ValueError, "did not complete within"),
            ):
                build_release.verify_xtb_worker(
                    Path("fixture-reshiki"), {"xtb": {"included": False}}
                )
            self.assertLess(time.monotonic() - started, 1.5)
            signal_group.assert_called_once()
            self.assertTrue(reader_reaped.wait(timeout=1))
        finally:
            exit_requested.set()
            for thread in set(threading.enumerate()) - threads:
                if thread.name == "ReShiki xTB check":
                    thread.join(timeout=2)

    @unittest.skipUnless(os.name == "posix", "POSIX inherited-pipe regression")
    def test_timeout_does_not_signal_group_after_worker_exits_with_inherited_stdout(self):
        replies = self.replies()
        replies["hello"].update(engine=None, methods=[])
        with self.assertRaisesRegex(ValueError, "did not complete within 1 seconds"):
            self.verify_fixture(replies=replies, included=False, mode="inherited_pipe")
        self.assertLess(self.verification_elapsed, 3)
        self.assertEqual(self.group_kills, [])

    @unittest.skipUnless(os.name == "posix", "POSIX inherited-pipe regression")
    def test_detached_descendant_cannot_stall_timeout_cleanup(self):
        replies = self.replies()
        replies["hello"].update(engine=None, methods=[])
        with self.assertRaisesRegex(ValueError, "did not complete within 1 seconds"):
            self.verify_fixture(
                replies=replies, included=False, mode="inherited_pipe", detached=True
            )
        self.assertLess(self.verification_elapsed, 3)

    def test_platform_without_process_groups_still_has_a_bounded_timeout(self):
        with self.assertRaisesRegex(ValueError, "did not complete within 1 seconds"):
            self.verify_fixture(mode="no_reply", portable=True)
        self.assertLess(self.verification_elapsed, 3)


class XtbReleaseCliTests(unittest.TestCase):
    def test_cli_checks_the_installed_executable_own_worker_and_pinned_energy(self):
        from build_release import XTB_WATER_ENERGY, verify_xtb_cli

        value = {
            "method": "gfn2",
            "engine": {"rev": AXTB_REV},
            "energy_hartree": XTB_WATER_ENERGY,
        }
        for energy in (XTB_WATER_ENERGY, XTB_WATER_ENERGY + 0.01):
            response = {"value": {**value, "energy_hartree": energy}}

            def calculate(command, **kwargs):
                self.assertEqual(
                    command[:5],
                    [
                        Path("installed/reshiki.exe").resolve(),
                        "--cli",
                        "calculate",
                        "--threads",
                        "1",
                    ],
                )
                self.assertTrue(command[5].is_file())
                self.assertIn(" ", kwargs["cwd"])
                self.assertIn("AXTB_LAPACK", kwargs["env"])
                return subprocess.CompletedProcess(command, 0, json.dumps(response), "")

            with patch("build_release.subprocess.run", side_effect=calculate):
                if energy == XTB_WATER_ENERGY:
                    verify_xtb_cli(
                        Path("installed/reshiki.exe"), {"xtb": {"included": True, "rev": AXTB_REV}}
                    )
                else:
                    with self.assertRaisesRegex(ValueError, "wrong method, engine or water energy"):
                        verify_xtb_cli(
                            Path("installed/reshiki.exe"),
                            {"xtb": {"included": True, "rev": AXTB_REV}},
                        )

    def test_unavailable_cli_prints_its_error_and_engine_free_packages_skip_it(self):
        from build_release import verify_xtb_cli

        with patch("build_release.subprocess.run") as run:
            verify_xtb_cli(Path("reshiki.exe"), {"xtb": {"included": False}})
            run.assert_not_called()
            run.return_value = subprocess.CompletedProcess([], 1, "", "worker could not start")
            with self.assertRaisesRegex(ValueError, "worker could not start"):
                verify_xtb_cli(Path("reshiki.exe"), {"xtb": {"included": True}})


class MacXtbDiskImageTests(unittest.TestCase):
    def test_installation_text_reuses_zip_xtb_notice_and_metadata(self):
        import installers
        from build_release import xtb_readme

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            folder = root / "reshiki-1.2.3-macos-arm64"
            folder.mkdir()
            xtb = dict(included=True, source_archive="reshiki-1.2.3-source.tar.gz")
            metadata = dict(
                version="1.2.3",
                platform="macos",
                architecture="arm64",
                release_tag="v1.2.3",
                xtb=xtb,
            )
            (folder / "build.json").write_text(json.dumps(metadata), encoding="utf-8")
            instructions = []

            def create(staging, output):
                instructions.append((staging / "Install ReShiki.txt").read_text(encoding="utf-8"))
                output.write_bytes(b"disk image fixture")

            with (
                patch("installers.run"),
                patch("installers.create_disk_image", side_effect=create),
                patch("installers.verify_mac_disk_image") as verify,
            ):
                output = installers.mac_disk_image(folder, root / "output")
            self.assertTrue(instructions[0].endswith(xtb_readme(xtb, "v1.2.3", "macos")))
            self.assertIn("ReShiki.app/Contents/Resources/Licenses/", instructions[0])
            self.assertIn("reshiki-1.2.3-source.tar.gz", instructions[0])
            self.assertIn(
                "https://github.com/Ameyanagi/ReShiki/releases/tag/v1.2.3", instructions[0]
            )
            verify.assert_called_once_with(output, False, architecture="arm64", metadata=metadata)


class TransientRetryTests(unittest.TestCase):
    @staticmethod
    def failure(command):
        return subprocess.CalledProcessError(68, command)

    def test_stapler_retries_transient_failures_then_succeeds(self):
        import sign_macos

        outcomes = [self.failure(["xcrun"]), self.failure(["xcrun"]), "stapled"]

        def fake(command):
            outcome = outcomes.pop(0)
            if isinstance(outcome, Exception):
                raise outcome
            return outcome

        sleeps = []
        with patch("sign_macos.run", side_effect=fake) as run, redirect_stdout(io.StringIO()):
            self.assertEqual(
                sign_macos.stapler("staple", "App.app", sleep=sleeps.append), "stapled"
            )
        self.assertEqual(run.call_count, 3)
        self.assertEqual(run.call_args.args[0], ["xcrun", "stapler", "staple", "App.app"])
        self.assertEqual(sleeps, [sign_macos.STAPLER_DELAY_SECONDS] * 2)

    def test_stapler_raises_after_the_last_attempt(self):
        import sign_macos

        with (
            patch("sign_macos.run", side_effect=self.failure(["xcrun"])) as run,
            redirect_stdout(io.StringIO()),
            self.assertRaises(subprocess.CalledProcessError),
        ):
            sign_macos.stapler("validate", "image.dmg", sleep=lambda _: None)
        self.assertEqual(run.call_count, sign_macos.STAPLER_ATTEMPTS)

    def test_disk_image_creation_retries_resource_busy(self):
        import installers

        # Each hdiutil step fails once, then passes.
        outcomes = [self.failure(["hdiutil"]), None, self.failure(["hdiutil"]), None]

        def fake(command):
            outcome = outcomes.pop(0)
            if isinstance(outcome, Exception):
                raise outcome

        waits = []
        log = io.StringIO()
        with (
            tempfile.TemporaryDirectory() as temporary,
            patch("installers.run", side_effect=fake) as run,
            patch("installers.shutil.copyfile") as copy,
            redirect_stdout(log),
        ):
            contents = Path(temporary)
            # A sparse file: 100 MiB long, almost no blocks on disk.
            with (contents / "reshiki").open("wb") as file:
                file.truncate(100 * 2**20)
            installers.create_disk_image(contents, Path("out.dmg"), sleep=waits.append)
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(
            [command[1] for command in commands], ["create", "create", "convert", "convert"]
        )
        create, convert = commands[1], commands[3]
        self.assertEqual(create[create.index("-srcfolder") + 1], contents)
        # Sized from file lengths with headroom, not from blocks on disk.
        self.assertEqual(create[create.index("-size") + 1], "142m")
        self.assertEqual(create[create.index("-format") + 1], "UDZO")
        # Each convert attempt reads its own fresh copy of the created image.
        self.assertEqual(
            [call.args for call in copy.call_args_list],
            [(create[-1], commands[2][2]), (create[-1], convert[2])],
        )
        self.assertEqual(len({create[-1], commands[2][2], convert[2]}), 3)
        # LZMA compression; every supported macOS (14 and later) opens it.
        self.assertEqual(convert[convert.index("-format") + 1], "ULMO")
        self.assertEqual(convert[convert.index("-o") + 1], Path("out.dmg"))
        for command in commands:
            self.assertIn("-ov", command)
        self.assertEqual(waits, [5, 5])
        self.assertIn("hdiutil convert succeeded on attempt 2", log.getvalue())

    def test_disk_image_creation_raises_after_the_last_attempt(self):
        import installers

        # "create" writes the image; every "convert" then fails.
        sources = []

        def fake(command):
            if command[1] == "create":
                command[-1].write_bytes(b"zlib image")
                return
            sources.append((command[2], command[2].read_bytes()))
            raise self.failure(["hdiutil"])

        waits = []
        with (
            patch("installers.run", side_effect=fake),
            redirect_stdout(io.StringIO()),
            self.assertRaises(subprocess.CalledProcessError),
        ):
            installers.create_disk_image(Path("contents"), Path("out.dmg"), sleep=waits.append)
        # Five attempts, each on its own real copy of the created image.
        self.assertEqual(len({source for source, _ in sources}), 5)
        self.assertEqual([content for _, content in sources], [b"zlib image"] * 5)
        self.assertEqual(waits, [5, 10, 20, 40])


if __name__ == "__main__":
    unittest.main()
