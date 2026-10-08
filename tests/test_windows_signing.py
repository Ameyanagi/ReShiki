"""Qualified bytes, provenance, and final installer checks survive signing stages."""

import hashlib
import json
import os
import struct
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import sign_windows_release as signing


def pe_fixture(architecture="x64"):
    data = bytearray(320)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 64)
    data[64:68] = b"PE\0\0"
    struct.pack_into("<H", data, 68, 0x8664 if architecture == "x64" else 0xAA64)
    struct.pack_into("<H", data, 84, 232)
    struct.pack_into("<H", data, 88, 0x20B)
    struct.pack_into("<I", data, 88 + 108, 16)
    return bytes(data)


def append_signature(data):
    result = bytearray(data)
    padding = (-len(result)) % 8
    result.extend(bytes(padding))
    offset = len(result)
    result.extend(b"certificate data")
    checksum, directory, _, _ = signing.pe_fields(data)
    struct.pack_into("<I", result, checksum, 123)
    struct.pack_into("<II", result, directory, offset, 16)
    return bytes(result)


class WindowsSigningTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.pin = "ab" * 32
        self.stem = "reshiki-1.2.3-windows-x64"
        self.metadata = {
            "version": "1.2.3",
            "platform": "windows",
            "architecture": "x64",
            "commit": "qualified-commit",
            "signed": False,
            "notarized": False,
        }
        self.addCleanup(patch.stopall)
        patch.object(signing, "version", return_value="1.2.3").start()
        patch.dict(os.environ, {"GITHUB_SHA": "qualified-commit"}).start()

    def qualified(self, *, extra=None, metadata=None):
        unsigned = self.root / "unsigned"
        unsigned.mkdir(exist_ok=True)
        source = unsigned / f"{self.stem}.zip"
        with zipfile.ZipFile(source, "w") as archive:
            archive.writestr(f"{self.stem}/build.json", json.dumps(metadata or self.metadata))
            archive.writestr(f"{self.stem}/reshiki.exe", pe_fixture())
            archive.writestr(f"{self.stem}/README.txt", "This build has no publisher signature.\n")
            if extra:
                archive.writestr(extra, "unexpected file")
        signing.checksum(source)
        return source

    def test_staging_rejects_changed_provenance_hash_and_archive_paths(self):
        for failure in ("checksum", "commit", "architecture", "path", "duplicate", "pin"):
            with self.subTest(failure=failure):
                for path in (self.root / "windows-signing", self.root / "signing-input"):
                    if path.exists():
                        import shutil

                        shutil.rmtree(path)
                metadata = dict(self.metadata)
                if failure in {"commit", "architecture"}:
                    metadata[failure] = "wrong"
                source = self.qualified(
                    metadata=metadata,
                    extra=f"{self.stem}/../escape" if failure == "path" else None,
                )
                if failure == "checksum":
                    source.write_bytes(source.read_bytes() + b"changed")
                if failure == "duplicate":
                    with zipfile.ZipFile(source, "a") as archive:
                        archive.writestr(f"{self.stem}/RESHIKI.EXE", b"ambiguous")
                    signing.checksum(source)
                pin = "" if failure == "pin" else self.pin
                with self.assertRaises(ValueError):
                    signing.prepare(self.root, "x64", "test-signing", pin)
                self.assertFalse((self.root / "signing-input/app/reshiki.exe").exists())
                self.assertFalse((self.root / "escape").exists())

    def test_signing_only_changes_authenticode_fields_and_appends_the_certificate(self):
        unsigned = self.root / "unsigned.exe"
        signed = self.root / "signed.exe"
        original = pe_fixture()
        unsigned.write_bytes(original)
        signed.write_bytes(append_signature(original))
        signing.verify_signature_only_change(unsigned, signed)
        for offset in (2, 70, 100, 319):
            with self.subTest(offset=offset):
                changed = bytearray(append_signature(original))
                changed[offset] ^= 1
                signed.write_bytes(changed)
                with self.assertRaises(ValueError):
                    signing.verify_signature_only_change(unsigned, signed)
        signed.write_bytes(append_signature(original) + b"unsigned overlay")
        with self.assertRaises(ValueError):
            signing.verify_signature_only_change(unsigned, signed)

    def test_signed_app_is_installed_and_final_bytes_are_checked_before_checksums(self):
        source = self.qualified()
        state = signing.prepare(self.root, "x64", "test-signing", self.pin)
        self.assertEqual(state["unsigned_sha256"], signing.digest(source))
        app = self.root / "signed-app/reshiki.exe"
        app.parent.mkdir()
        app.write_bytes(append_signature(pe_fixture()))
        events = []

        def verify_signature(path, policy, certificate, version):
            events.append("verify-signature")
            return {
                "certificate_sha256": certificate,
                "file_sha256": signing.digest(path),
                "timestamp_subject": "Timestamp fixture",
            }

        def installer(folder, output):
            self.assertEqual((folder / "reshiki.exe").read_bytes(), app.read_bytes())
            metadata = json.loads((folder / "build.json").read_text())
            self.assertTrue(metadata["signed"])
            self.assertFalse(metadata["signing"]["publicly_trusted"])
            self.assertEqual(metadata["unsigned_sha256"], signing.digest(source))
            output.mkdir(parents=True)
            result = output / f"{self.stem}-setup.exe"
            result.write_bytes(pe_fixture())
            return result

        with (
            patch.object(signing, "verify_signature", side_effect=verify_signature),
            patch("build_release.platform.system", return_value="Windows"),
            patch.object(
                signing, "verify_archive", side_effect=lambda *_a, **_k: events.append("portable")
            ),
            patch.object(signing, "build_installer", side_effect=installer),
            patch.object(
                signing, "verify_windows_installer", side_effect=lambda *_: events.append("install")
            ),
            patch.object(
                signing,
                "verify_windows_upgrade_with_running_agent",
                side_effect=lambda *_: events.append("upgrade"),
            ),
        ):
            original_installer = signing.package(self.root, "x64", "test-signing", self.pin)
            signed_installer = self.root / "signed-installer" / original_installer.name
            signed_installer.parent.mkdir()
            signed_installer.write_bytes(append_signature(original_installer.read_bytes()))
            output = signing.verify(self.root, "x64", "test-signing", self.pin)
        self.assertEqual(output.read_bytes(), signed_installer.read_bytes())
        digest = hashlib.sha256(output.read_bytes()).hexdigest()
        self.assertEqual(Path(str(output) + ".sha256").read_text(), f"{digest}  {output.name}\n")
        self.assertEqual(
            events,
            [
                "verify-signature",
                "portable",
                "verify-signature",
                "verify-signature",
                "install",
                "upgrade",
            ],
        )


if __name__ == "__main__":
    unittest.main()
