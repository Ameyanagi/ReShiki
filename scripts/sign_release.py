"""Sign a qualified macOS archive from the same workflow, without rebuilding it."""

import hashlib
import json
import tempfile
from pathlib import Path

from build_release import ROOT, archive, run, verify_archive, version
from installers import mac_disk_image
from sign_macos import sign_and_notarize


def main():
    inputs = list((ROOT / "unsigned").glob("*.zip"))
    if len(inputs) != 1:
        raise ValueError("Expected exactly one macOS build archive")
    source = inputs[0]
    with source.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    if (
        Path(str(source) + ".sha256").read_text(encoding="utf-8").strip()
        != f"{digest}  {source.name}"
    ):
        raise ValueError("Archive checksum mismatch")
    with tempfile.TemporaryDirectory(prefix="reshiki-release-sign-") as temporary:
        directory = Path(temporary)
        run(["ditto", "-x", "-k", source, directory])
        folder = directory / source.stem
        metadata = json.loads((folder / "build.json").read_text(encoding="utf-8"))
        import os

        if (
            metadata.get("version") != version()
            or metadata.get("commit") != os.environ["GITHUB_SHA"]
        ):
            raise ValueError("Archive source does not match this release run")
        sign_and_notarize(folder / "ReShiki.app")
        metadata.update(signed=True, notarized=True, unsigned_sha256=digest)
        (folder / "build.json").write_text(json.dumps(metadata, indent=2) + "\n")
        readme = folder / "README.txt"
        readme.write_text(
            readme.read_text(encoding="utf-8").replace(
                "This build has no publisher signature.",
                "macOS application signed with Developer ID and notarized by Apple.",
            ),
            encoding="utf-8",
        )
        output = archive(folder, ROOT / "dist/releases" / source.stem)
        verify_archive(output, signed=True)
        with output.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        Path(str(output) + ".sha256").write_text(
            f"{digest}  {output.name}\n", encoding="ascii", newline="\n"
        )
        mac_disk_image(folder, ROOT / "dist/releases", signed=True)


if __name__ == "__main__":
    main()
