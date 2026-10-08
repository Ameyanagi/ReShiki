"""Publishing must reject incomplete/unsigned assets and link each exact filename."""

import hashlib
import json
import re
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from urllib.parse import unquote, urlparse

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from release_downloads import filenames, prepare, release_version


class ReleaseDownloadTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)

    def assets(self, version):
        for name in filenames(version):
            path = self.directory / name
            if any(system in name for system in ("-macos-", "-windows-")) and name.endswith(".zip"):
                architecture = "arm64" if "-arm64" in name else "x64"
                macos = "-macos-" in name
                with zipfile.ZipFile(path, "w") as archive:
                    archive.writestr(
                        f"{path.stem}/build.json",
                        json.dumps(
                            dict(
                                version=version,
                                platform="macos" if macos else "windows",
                                architecture=architecture,
                                signed=macos,
                                notarized=macos,
                            )
                        ),
                    )
            else:
                path.write_bytes(name.encode("ascii"))
            self.checksum(path)

    @staticmethod
    def checksum(path):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        Path(str(path) + ".sha256").write_text(f"{digest}  {path.name}\n", encoding="ascii")

    def test_stable_and_nightly_tables_link_all_ten_verified_assets(self):
        for tag in ("v0.9.1", "nightly-0.9.1-nightly.20260929.36501221724.1"):
            with self.subTest(tag=tag):
                for path in self.directory.iterdir():
                    path.unlink()
                version = release_version(tag)
                self.assets(version)
                notes = prepare(self.directory, "Ameyanagi/ReShiki", tag)
                links = re.findall(r"\]\((https://github.com/[^)]+/download/[^)]+)\)", notes)
                linked = {unquote(urlparse(url).path.rsplit("/", 1)[-1]) for url in links}
                self.assertEqual(linked, set(filenames(version)) | {"SHA256SUMS"})
                self.assertEqual(len((self.directory / "SHA256SUMS").read_text().splitlines()), 10)
                self.assertIn("Windows and Linux downloads are unsigned", notes)
                self.assertIn("/releases/latest", notes)

    def test_missing_installer_or_unexpected_old_asset_cannot_publish(self):
        self.assets("0.9.1")
        missing = self.directory / "reshiki-0.9.1-windows-arm64-setup.exe"
        missing.unlink()
        with self.assertRaisesRegex(ValueError, "missing="):
            prepare(self.directory, "Ameyanagi/ReShiki", "v0.9.1")
        self.assets("0.9.1")
        (self.directory / "reshiki-0.9.0-linux-x64.tar.gz").touch()
        with self.assertRaisesRegex(ValueError, "unexpected="):
            prepare(self.directory, "Ameyanagi/ReShiki", "v0.9.1")

    def test_checksum_and_signed_mac_metadata_are_required(self):
        self.assets("0.9.1")
        damaged = self.directory / "reshiki-0.9.1-windows-x64.zip"
        damaged.write_bytes(b"damaged")
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            prepare(self.directory, "Ameyanagi/ReShiki", "v0.9.1")
        self.assets("0.9.1")
        unsigned = self.directory / "reshiki-0.9.1-macos-arm64.zip"
        with zipfile.ZipFile(unsigned, "w") as archive:
            archive.writestr(
                f"{unsigned.stem}/build.json", json.dumps({"version": "0.9.1", "signed": False})
            )
        self.checksum(unsigned)
        with self.assertRaisesRegex(ValueError, "not the signed, notarized release"):
            prepare(self.directory, "Ameyanagi/ReShiki", "v0.9.1")

    def test_unsafe_tags_and_repositories_are_rejected(self):
        for tag in ("../v0.9.1", "v0.9.1/evil", "nightly-0.9.1", "v0.9.1\n"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release_version(tag)
        with self.assertRaises(ValueError):
            prepare(self.directory, "owner/repo/../../other", "v0.9.1")

    def signed_windows_assets(self, policy="release-signing", *, tampered=False):
        certificate = "ab" * 32
        for architecture in ("x64", "arm64"):
            name = f"reshiki-0.9.1-windows-{architecture}"
            path = self.directory / f"{name}.zip"
            binary = b"signed Windows fixture"
            metadata = {
                "version": "0.9.1",
                "platform": "windows",
                "architecture": architecture,
                "signed": True,
                "notarized": False,
                "signing": {
                    "provider": "SignPath",
                    "policy": policy,
                    "certificate_sha256": certificate,
                    "publicly_trusted": policy == "release-signing",
                    "application": {
                        "certificate_sha256": certificate,
                        "file_sha256": hashlib.sha256(binary).hexdigest(),
                        "timestamp_subject": "Timestamp authority fixture",
                    },
                },
            }
            with zipfile.ZipFile(path, "w") as archive:
                archive.writestr(f"{name}/build.json", json.dumps(metadata))
                archive.writestr(f"{name}/reshiki.exe", b"tampered" if tampered else binary)
            self.checksum(path)

    def test_enabled_windows_signing_requires_production_evidence(self):
        self.assets("0.9.1")
        with self.assertRaisesRegex(ValueError, "signing status"):
            prepare(self.directory, "Ameyanagi/ReShiki", "v0.9.1", "required")
        self.signed_windows_assets()
        notes = prepare(self.directory, "Ameyanagi/ReShiki", "v0.9.1", "required")
        self.assertIn("signed with SignPath Foundation", notes)
        self.signed_windows_assets(tampered=True)
        with self.assertRaisesRegex(ValueError, "signature evidence"):
            prepare(self.directory, "Ameyanagi/ReShiki", "v0.9.1", "required")

    def test_test_signatures_cannot_publish_in_either_mode(self):
        self.assets("0.9.1")
        self.signed_windows_assets("test-signing")
        for mode in ("required", "unsigned"):
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                prepare(self.directory, "Ameyanagi/ReShiki", "v0.9.1", mode)


if __name__ == "__main__":
    unittest.main()
