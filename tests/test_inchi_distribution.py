"""Pinned-source staging and native-helper release checks without application UI."""

import contextlib
import hashlib
import io
import json
import os
import shutil
import stat
import struct
import subprocess
import sys
import tempfile
import types
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import build_release
import inchi_source


class SourceTests(unittest.TestCase):
    def archive(self, root, entries=None):
        archive = root / "official.zip"
        entries = entries or {"INCHI-1-SRC/INCHI_BASE/src/test.c": b"audited source\n"}
        with zipfile.ZipFile(archive, "w") as stream:
            for name, data in entries.items():
                # Preserve deliberately invalid names in the fixture. ZipInfo
                # construction otherwise normalizes backslashes on Windows.
                entry = zipfile.ZipInfo(name)
                entry.filename = entry.orig_filename = name
                stream.writestr(entry, data)
        reference = {
            "inchi_version": "1.07.3",
            "archive_url": "https://github.com/IUPAC-InChI/InChI/releases/download/v1.07.3/INCHI-1-SRC.zip",
            "archive_root": "INCHI-1-SRC",
            "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
            "files": {"INCHI_BASE/src/test.c": hashlib.sha256(b"audited source\n").hexdigest()},
        }
        return archive, reference

    def test_local_archive_stages_verified_files_and_reuses_verified_cache(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, reference = self.archive(root)
            with (
                patch("inchi_source.manifest", return_value=reference),
                patch(
                    "inchi_source.urllib.request.urlopen",
                    side_effect=AssertionError("Unexpected network"),
                ),
            ):
                source = inchi_source.prepare_source(root / "cache", archive=archive)
                self.assertEqual(
                    (source / "INCHI_BASE/src/test.c").read_bytes(), b"audited source\n"
                )
                self.assertEqual(
                    source, inchi_source.prepare_source(root / "cache", archive=archive)
                )
                (source / "INCHI_BASE/src/test.c").write_text("changed")
                with self.assertRaisesRegex(ValueError, "source changed"):
                    inchi_source.prepare_source(root / "cache", archive=archive)

    def test_local_source_is_checked_without_fetch_or_extraction(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, reference = self.archive(root)
            source = inchi_source.extract_archive(archive, root / "stage", reference)
            with patch("inchi_source.manifest", return_value=reference):
                self.assertEqual(
                    source, inchi_source.prepare_source(root / "unused", source=source)
                )
            self.assertFalse((root / "unused").exists())

    def test_pruned_managed_cache_is_rebuilt_from_verified_download(self):
        for damage in ("one file", "all files", "source directory"):
            with self.subTest(damage=damage), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                entries = {
                    "INCHI-1-SRC/INCHI_BASE/src/test.c": b"audited source\n",
                    "INCHI-1-SRC/INCHI_API/libinchi/src/other.c": b"other source\n",
                }
                archive, reference = self.archive(root, entries)
                reference["files"]["INCHI_API/libinchi/src/other.c"] = hashlib.sha256(
                    b"other source\n"
                ).hexdigest()
                with patch("inchi_source.manifest", return_value=reference):
                    source = inchi_source.prepare_source(root / "cache", archive=archive)
                    if damage == "source directory":
                        shutil.rmtree(source)
                    else:
                        (source / "INCHI_BASE/src/test.c").unlink()
                        if damage == "all files":
                            # rust-cache prunes non-Cargo files while leaving
                            # their directory tree and CACHEDIR.TAG behind.
                            for file in source.rglob("*"):
                                if file.is_file():
                                    file.unlink()
                            (source.parent / "CACHEDIR.TAG").write_text("retained cache tag")
                    response = io.BytesIO(archive.read_bytes())
                    response.url = "https://release-assets.githubusercontent.com/official-object"
                    with patch(
                        "inchi_source.urllib.request.urlopen", return_value=response
                    ) as request:
                        repaired = inchi_source.prepare_source(root / "cache", fetch=True)
                        self.assertEqual(repaired, source)
                        for name, expected in entries.items():
                            self.assertEqual((source.parent / name).read_bytes(), expected)
                        # A valid restored cache needs no second download.
                        self.assertEqual(
                            repaired, inchi_source.prepare_source(root / "cache", fetch=True)
                        )
                        request.assert_called_once_with(reference["archive_url"], timeout=60)
                self.assertEqual([p.resolve() for p in (root / "cache").iterdir()], [source.parent])

    def test_missing_managed_file_can_be_restored_from_local_archive(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, reference = self.archive(root)
            with (
                patch("inchi_source.manifest", return_value=reference),
                patch("inchi_source.download_archive", side_effect=AssertionError("Network")),
            ):
                source = inchi_source.prepare_source(root / "cache", archive=archive)
                (source / "INCHI_BASE/src/test.c").unlink()
                restored = inchi_source.prepare_source(root / "cache", archive=archive)
                self.assertEqual(restored, source)
                self.assertEqual(
                    (source / "INCHI_BASE/src/test.c").read_bytes(), b"audited source\n"
                )

    def test_missing_file_does_not_hide_corrupt_managed_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, reference = self.archive(root)
            with (
                patch("inchi_source.manifest", return_value=reference),
                patch("inchi_source.download_archive") as download,
            ):
                source = inchi_source.prepare_source(root / "cache", archive=archive)
                reference["files"] = {"missing.c": "0" * 64, **reference["files"]}
                changed = source / "INCHI_BASE/src/test.c"
                changed.write_bytes(b"modified source")
                with self.assertRaisesRegex(ValueError, "source changed"):
                    inchi_source.prepare_source(root / "cache", fetch=True)
                self.assertEqual(changed.read_bytes(), b"modified source")
                self.assertFalse((source / "missing.c").exists())
                download.assert_not_called()

    def test_incomplete_explicit_source_fails_without_repair_or_cache_creation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, reference = self.archive(root)
            source = inchi_source.extract_archive(archive, root / "stage", reference)
            file = source / "INCHI_BASE/src/test.c"
            with (
                patch("inchi_source.manifest", return_value=reference),
                patch("inchi_source.download_archive") as download,
            ):
                file.write_bytes(b"modified source")
                with self.assertRaisesRegex(ValueError, "source changed"):
                    inchi_source.prepare_source(root / "unused", source=source)
                self.assertEqual(file.read_bytes(), b"modified source")
                file.unlink()
                with self.assertRaisesRegex(inchi_source.MissingSourceError, "Missing official"):
                    inchi_source.prepare_source(root / "unused", source=source)
                self.assertFalse(file.exists())
                download.assert_not_called()
            self.assertFalse((root / "unused").exists())

    def test_failed_recovery_retains_incomplete_tree_and_archive_guards(self):
        for failure in ("download checksum", "source checksum", "local archive checksum"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive, reference = self.archive(root)
                with patch("inchi_source.manifest", return_value=reference):
                    source = inchi_source.prepare_source(root / "cache", archive=archive)
                    (source / "INCHI_BASE/src/test.c").unlink()
                    sentinel = source / "CACHEDIR.TAG"
                    sentinel.write_bytes(b"existing incomplete tree")
                    if failure == "source checksum":
                        reference["files"]["INCHI_BASE/src/test.c"] = "0" * 64
                    data = archive.read_bytes() if failure == "source checksum" else b"changed"
                    response = io.BytesIO(data)
                    response.url = "https://release-assets.githubusercontent.com/official-object"
                    if failure == "local archive checksum":
                        archive.write_bytes(data)
                        choice = {"archive": archive}
                    else:
                        choice = {"fetch": True}
                    with (
                        patch("inchi_source.urllib.request.urlopen", return_value=response),
                        self.assertRaisesRegex(ValueError, "checksum mismatch|source changed"),
                    ):
                        inchi_source.prepare_source(root / "cache", **choice)
                    self.assertEqual(sentinel.read_bytes(), b"existing incomplete tree")
                    self.assertFalse((source / "INCHI_BASE/src/test.c").exists())
                    self.assertEqual(
                        [p.resolve() for p in (root / "cache").iterdir()], [source.parent]
                    )

    def test_failed_replacement_rename_restores_incomplete_cache(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, reference = self.archive(root)
            with patch("inchi_source.manifest", return_value=reference):
                source = inchi_source.prepare_source(root / "cache", archive=archive)
                (source / "INCHI_BASE/src/test.c").unlink()
                sentinel = source / "CACHEDIR.TAG"
                sentinel.write_bytes(b"previous incomplete tree")
                rename = Path.rename
                failure = PermissionError("Replacement directory is locked")

                def locked(path, destination):
                    if path.name == "extracted":
                        raise failure
                    return rename(path, destination)

                with (
                    patch.object(Path, "rename", locked),
                    self.assertRaises(PermissionError) as ctx,
                ):
                    inchi_source.prepare_source(root / "cache", archive=archive)
                self.assertIs(ctx.exception, failure)
                self.assertEqual(sentinel.read_bytes(), b"previous incomplete tree")
                self.assertFalse((source / "INCHI_BASE/src/test.c").exists())
                self.assertEqual([p.resolve() for p in (root / "cache").iterdir()], [source.parent])
                self.assertEqual(
                    inchi_source.prepare_source(root / "cache", archive=archive), source
                )

    def test_checksum_and_source_mismatch_leave_no_reusable_cache(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, reference = self.archive(root)
            for changed in [
                {**reference, "archive_sha256": "0" * 64},
                {**reference, "files": {"INCHI_BASE/src/test.c": "0" * 64}},
            ]:
                with patch("inchi_source.manifest", return_value=changed):
                    with self.assertRaises(ValueError):
                        inchi_source.prepare_source(root / "cache", archive=archive)
                    cache = root / "cache"
                    self.assertFalse(cache.exists() and any(cache.iterdir()))

    def test_archive_rejects_traversal_absolute_paths_duplicates_and_links_before_writes(self):
        invalid = [
            "../escape",
            "INCHI-1-SRC/../escape",
            "/INCHI-1-SRC/file",
            "INCHI-1-SRC/x\\y",
            "INCHI-1-SRC/x:y",
            "INCHI-1-SRC/x//y",
        ]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in invalid:
                archive, reference = self.archive(root, {name: b"invalid"})
                with self.assertRaisesRegex(ValueError, "Invalid InChI archive entry"):
                    inchi_source.extract_archive(archive, root / "stage", reference)
                self.assertFalse((root / "stage").exists())
            archive, reference = self.archive(root, {"INCHI-1-SRC/X": b"a", "INCHI-1-SRC/x": b"b"})
            with self.assertRaises(ValueError):
                inchi_source.extract_archive(archive, root / "stage", reference)
            link = zipfile.ZipInfo("INCHI-1-SRC/link")
            link.create_system = 3
            link.external_attr = (stat.S_IFLNK | 0o777) << 16
            with zipfile.ZipFile(archive, "w") as stream:
                stream.writestr(link, "../../outside")
            reference["archive_sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
            with self.assertRaisesRegex(ValueError, "Unsupported InChI archive entry"):
                inchi_source.extract_archive(archive, root / "stage", reference)

    def test_extraction_budgets_and_explicit_source_choice(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, reference = self.archive(root)
            for limit in ["MAX_ARCHIVE_BYTES", "MAX_EXTRACTED_BYTES", "MAX_ENTRIES"]:
                with patch.object(inchi_source, limit, 0), self.assertRaises(ValueError):
                    inchi_source.extract_archive(archive, root / "stage", reference)
            for kwargs in [
                {},
                {"source": root, "fetch": True},
                {"source": root, "archive": archive},
            ]:
                with self.assertRaisesRegex(ValueError, "exactly one"):
                    inchi_source.prepare_source(root / "stage", **kwargs)

    def test_explicit_download_uses_only_pinned_url_and_checksum(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, reference = self.archive(root)
            response = io.BytesIO(archive.read_bytes())
            response.url = "https://release-assets.githubusercontent.com/official-object"
            with (
                patch("inchi_source.manifest", return_value=reference),
                patch("inchi_source.urllib.request.urlopen", return_value=response) as request,
            ):
                source = inchi_source.prepare_source(root / "cache", fetch=True)
                self.assertTrue(source.is_dir())
                request.assert_called_once_with(reference["archive_url"], timeout=60)
            with self.assertRaisesRegex(ValueError, "pinned official release"):
                inchi_source.download_archive(
                    root / "bad", {**reference, "archive_url": "https://example.com/source.zip"}
                )
            for url, limit in [("http://example.com", 1000), ("https://github.com", 1)]:
                response = io.BytesIO(b"too large")
                response.url = url
                with (
                    patch("inchi_source.urllib.request.urlopen", return_value=response),
                    patch("inchi_source.MAX_ARCHIVE_BYTES", limit),
                    self.assertRaises(ValueError),
                ):
                    inchi_source.download_archive(root / f"bad-{limit}", reference)


class HelperTests(unittest.TestCase):
    @staticmethod
    def reply(version="1.07.3", inchi="InChI=1S/CH4/h1H4", extra=b""):
        def text(value):
            data = value.encode()
            return struct.pack("<I", len(data)) + data

        return (
            b"RSHINCHI\x02\x00"
            + text(version)
            + struct.pack("<Hh", 0, 0)
            + text(inchi)
            + text("")
            + text("")
            + text("AuxInfo=1/0/N:1/rA:1C/rB:/rC:;")
            + extra
        )

    def test_archive_smoke_checks_protocol_version_and_chemical_output(self):
        valid = self.reply()
        for response in [valid[:n] for n in range(len(valid))] + [
            self.reply(version="wrong"),
            self.reply(inchi="wrong"),
            self.reply(extra=b"extra"),
        ]:
            with patch(
                "build_release.run",
                return_value=subprocess.CompletedProcess([], 0, stdout=response),
            ):
                with self.assertRaises((ValueError, UnicodeError)):
                    build_release.verify_inchi_helper(Path("helper"), "1.07.3")
        with patch(
            "build_release.run", return_value=subprocess.CompletedProcess([], 0, stdout=valid)
        ) as run:
            build_release.verify_inchi_helper(Path("helper"), "1.07.3")
            request = run.call_args.kwargs["input"]
            self.assertEqual(request[:12], b"RSHINCHI\x02\x00\x01\x00")
            self.assertEqual(run.call_args.args[0], [Path("helper")])
            self.assertEqual(run.call_args.kwargs["timeout"], 15)

    def test_build_source_choice_is_required_before_any_download_or_compile(self):
        with patch("build_release.run") as run:
            for kwargs in [{}, {"source": Path("src"), "fetch": True}]:
                with self.assertRaisesRegex(ValueError, "Choose --inchi-source"):
                    build_release.prepare_inchi_helper("x86_64-unknown-linux-gnu", **kwargs)
            run.assert_not_called()

    def test_explicit_native_source_build_uses_target_and_production_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = "aarch64-pc-windows-msvc"
            output = root / target / "inchi-helper/reshiki-inchi-helper.exe"
            with (
                patch("build_release.target_directory", return_value=root),
                patch("inchi_source.prepare_source", return_value=root / "source") as source,
                patch("build_release.run") as run,
                patch("build_release.verify_inchi_build", return_value=(output, {})) as verify,
            ):
                result = build_release.prepare_inchi_helper(target, archive=root / "official.zip")
            self.assertEqual(result, (output, {}))
            source.assert_called_once_with(
                root / "inchi-sources", source=None, archive=root / "official.zip", fetch=False
            )
            command = run.call_args.args[0]
            self.assertIn("--production", command)
            self.assertEqual(command[command.index("--target") + 1], target)
            self.assertEqual(command[command.index("--output") + 1], output.parent)
            verify.assert_called_once_with(output, target)

    def test_prebuilt_helper_checks_all_native_targets_and_source_identity(self):
        reference = inchi_source.manifest()
        bridges = {
            relative: hashlib.sha256((build_release.ROOT / relative).read_bytes()).hexdigest()
            for relative in (
                "tools/inchi-helper/main.cpp",
                "tools/inchi-helper/arena.cpp",
                "tools/inchi-helper/arena.h",
                "tools/inchi-helper/allocator_redirect.h",
                "scripts/build_inchi_helper.py",
                "scripts/inchi_source_patch.py",
                "tools/inchi-helper/source-patches.json",
            )
        }
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for target, (system, architecture) in build_release.RELEASE_TARGETS.items():
                with self.subTest(target=target):
                    image = bytearray(64)
                    if system == "windows":
                        image[:2] = b"MZ"
                        image[60:64] = (64).to_bytes(4, "little")
                        image += b"PE\0\0" + (
                            0xAA64 if architecture == "arm64" else 0x8664
                        ).to_bytes(2, "little")
                    elif system == "linux":
                        image[:6] = b"\x7fELF\x02\x01"
                        image[18:20] = (183 if architecture == "arm64" else 62).to_bytes(
                            2, "little"
                        )
                    else:
                        image[:8] = b"\xcf\xfa\xed\xfe" + (0x0100000C).to_bytes(4, "little")
                    binary = root / build_release.inchi_helper_name(system)
                    binary.write_bytes(image)
                    metadata = {
                        "inchi_version": reference["inchi_version"],
                        "archive_sha256": reference["archive_sha256"],
                        "manifest_sha256": hashlib.sha256(
                            inchi_source.MANIFEST.read_bytes()
                        ).hexdigest(),
                        "executable_sha256": hashlib.sha256(image).hexdigest(),
                        "target": target,
                        "bridge_sources": bridges,
                        "patched_source_hashes": {
                            relative: item["patched_sha256"]
                            for relative, item in json.loads(
                                (
                                    build_release.ROOT / "tools/inchi-helper/source-patches.json"
                                ).read_text()
                            )["files"].items()
                        },
                    }
                    record = root / "build.json"
                    record.write_text(json.dumps(metadata))
                    helper, details = build_release.verify_inchi_build(binary, target)
                    self.assertEqual(helper, binary.resolve())
                    self.assertEqual(details["version"], "1.07.3")
                    for key in (
                        "target",
                        "manifest_sha256",
                        "executable_sha256",
                        "archive_sha256",
                        "patched_source_hashes",
                    ):
                        record.write_text(json.dumps({**metadata, key: "wrong"}))
                        with self.assertRaisesRegex(ValueError, "build metadata"):
                            build_release.verify_inchi_build(binary, target)
                    record.write_text(json.dumps({**metadata, "bridge_sources": {}}))
                    with self.assertRaisesRegex(ValueError, "bridge source changed"):
                        build_release.verify_inchi_build(binary, target)
                    binary.write_bytes(b"incorrect architecture")
                    metadata["executable_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
                    record.write_text(json.dumps(metadata))
                    with self.assertRaisesRegex(ValueError, "executable"):
                        build_release.verify_inchi_build(binary, target)

    def test_mac_helper_is_copied_before_bundle_signing_and_signed_inside_out(self):
        import sign_macos

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "custom-target/debug/reshiki"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"\xcf\xfa\xed\xfe")
            helper = root / "reshiki-inchi-helper"
            helper.write_bytes(b"\xcf\xfa\xed\xfe" + b"native helper")
            icon = root / "assets/branding/reshiki.icns"
            icon.parent.mkdir(parents=True)
            icon.write_bytes(b"icns fixture")
            app = root / "ReShiki.app"
            legacy = app / "Contents/Resources/chemistry/engine"
            legacy.mkdir(parents=True)
            (legacy / "worker.py").write_text("obsolete")
            copied = app / "Contents/MacOS/reshiki-inchi-helper"

            def run(command, **_kwargs):
                if command[0] == "swiftc":
                    Path(command[command.index("-o") + 1]).write_bytes(b"\xcf\xfa\xed\xfe")
                elif command[0] == "codesign":
                    self.assertEqual(copied.read_bytes(), helper.read_bytes())
                    self.assertEqual(
                        (app / "Contents/Resources/ReShiki.icns").read_bytes(), icon.read_bytes()
                    )
                    self.assertFalse(legacy.parent.exists())
                    self.assertTrue((app / "Contents/Resources/Licenses/NOTICE").is_file())

            with (
                patch("build_release.ROOT", root),
                patch("build_release.version", return_value="1.2.3"),
                patch("build_release.target_directory", return_value=root / "custom-target"),
                patch("build_release.run", side_effect=run),
                patch("build_release.notices") as notices,
            ):

                def write_notice(destination):
                    destination.mkdir(parents=True)
                    (destination / "NOTICE").write_text("Native licenses")

                notices.side_effect = write_notice
                build_release.mac_bundle(app, "debug", inchi_helper=helper)
                notices.assert_called_once_with(app / "Contents/Resources/Licenses")
            with (
                patch(
                    "sign_macos.signing_keychain",
                    return_value=contextlib.nullcontext(root / "keychain"),
                ),
                patch.dict(os.environ, {"MACOS_SIGNING_IDENTITY": "fixture identity"}),
                patch("sign_macos.run") as sign,
                patch("sign_macos.notarize"),
                patch("sign_macos.verify_app"),
            ):
                sign_macos.sign_and_notarize(app)
            signed = [
                call.args[0][-1]
                for call in sign.call_args_list
                if call.args[0][:2] == ["codesign", "--force"]
            ]
            self.assertIn(copied, signed)
            self.assertLess(signed.index(copied), signed.index(app))

    def test_mac_drag_to_install_verifies_the_relocated_helper(self):
        import installers

        def run(command, **_kwargs):
            if command[:2] == ["hdiutil", "attach"]:
                mount = Path(command[command.index("-mountpoint") + 1])
                executable = mount / "ReShiki.app/Contents/MacOS/reshiki"
                executable.parent.mkdir(parents=True)
                header = b"\xcf\xfa\xed\xfe" + (0x0100000C).to_bytes(4, "little")
                executable.write_bytes(header)
                executable.with_name("reshiki-inchi-helper").write_bytes(header)
                (mount / "Applications").symlink_to("/Applications")
            elif command[0] == "ditto":
                shutil.copytree(command[1], command[2])

        checked = []

        def helper(binary, version):
            self.assertTrue(binary.is_file())
            self.assertIn("Installed/ReShiki.app/Contents/MacOS", binary.as_posix())
            self.assertEqual(version, "1.07.3")
            checked.append(binary)

        with (
            patch("installers.run", side_effect=run),
            patch("installers.verify_inchi_helper", side_effect=helper),
            patch("installers.verify_runtime") as runtime,
        ):
            installers.verify_mac_disk_image(Path("fixture.dmg"), False)
        self.assertEqual(len(checked), 1)
        runtime.assert_called_once_with(checked[0].with_name("reshiki"), checked[0].parents[2])

    def test_windows_setup_and_upgrade_verify_the_relocated_helper(self):
        import installers

        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "source"
            source.mkdir()
            header = bytearray(64)
            header[:2] = b"MZ"
            header[60:64] = (64).to_bytes(4, "little")
            header += b"PE\0\0" + (0xAA64).to_bytes(2, "little")
            (source / "reshiki.exe").write_bytes(header)
            (source / "reshiki-inchi-helper.exe").write_bytes(header)
            (source / "build.json").write_text(json.dumps({"architecture": "arm64"}))
            installed = []

            def run(command, **_kwargs):
                executable = Path(command[0])
                if executable.name == "setup.exe":
                    destination = Path(
                        next(
                            arg.removeprefix("/DIR=")
                            for arg in command
                            if str(arg).startswith("/DIR=")
                        )
                    )
                    if installed:
                        self.assertEqual(
                            (destination / "chemistry/engine/worker.py").read_text(),
                            "Legacy worker",
                        )
                        # Simulate the installer's app-owned cleanup, preserving user sentinels.
                        shutil.rmtree(destination / "chemistry")
                    shutil.copytree(source, destination, dirs_exist_ok=True)
                    (destination / "unins000.exe").touch()
                    installed.append(destination)
                elif executable.name == "unins000.exe":
                    shutil.rmtree(executable.parent)
                else:
                    return subprocess.CompletedProcess(
                        command, 0, stdout='{"analysis":{"smiles":"CCO"}}'
                    )

            registry = types.SimpleNamespace(
                HKEY_CURRENT_USER=1,
                OpenKey=lambda *_: contextlib.nullcontext(1),
                QueryValueEx=lambda *_: (f'"{installed[-1] / "reshiki.exe"}" --ole-server', 0),
            )
            checked = []

            def helper(binary, version):
                self.assertEqual(binary, installed[-1] / "reshiki-inchi-helper.exe")
                self.assertTrue(binary.is_file())
                self.assertEqual(version, "1.07.3")
                checked.append(binary)

            with (
                patch("installers.sys.platform", "win32"),
                patch.dict(sys.modules, {"winreg": registry}),
                patch("installers.run", side_effect=run),
                patch("installers.verify_inchi_helper", side_effect=helper),
                patch("installers.verify_runtime") as runtime,
            ):
                installers.verify_windows_installer(Path("setup.exe"), source)
            self.assertEqual(runtime.call_args.args, (installed[-1] / "reshiki.exe", installed[-1]))
            self.assertEqual(
                runtime.call_args.kwargs["user_data"], installed[-1].parent / "User data"
            )
            self.assertEqual(len(installed), 2)
            self.assertEqual(len(checked), 1)

    def test_packaged_native_helper_executes_when_supplied_for_validation(self):
        binary = os.environ.get("RESHIKI_TEST_PACKAGED_INCHI_HELPER")
        if binary is None:
            self.skipTest("Optional independently built native package helper")
        build_release.verify_inchi_helper(Path(binary), "1.07.3")


if __name__ == "__main__":
    unittest.main()
