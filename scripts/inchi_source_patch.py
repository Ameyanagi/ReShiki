"""Apply the recorded string-buffer repair to a verified private build copy."""

import hashlib
import json
import shutil
import tempfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
PATCHES = ROOT / "tools/inchi-helper/source-patches.json"


def patch_manifest():
    return json.loads(PATCHES.read_text(encoding="utf-8"))


def stage_source(source, output, reference, patches=None):
    patches = patch_manifest() if patches is None else patches
    if patches["inchi_version"] != reference["inchi_version"]:
        raise ValueError("InChI source patch version mismatch")
    if not patches["files"].keys() <= reference["files"].keys():
        raise ValueError("InChI patch targets an unverified source file")
    # Read and validate before creating a private directory. No source file is
    # changed, including on failure; the build never edits the audited checkout.
    files = {}
    for relative, expected in reference["files"].items():
        path = PurePosixPath(relative)
        if (
            path.is_absolute()
            or path.as_posix() != relative
            or any(part in (".", "..") for part in relative.split("/"))
            or any(char in relative for char in ("\\", ":", "\0"))
        ):
            raise ValueError(f"Invalid InChI source path: {relative}")
        data = (Path(source) / relative).read_bytes()
        if hashlib.sha256(data).hexdigest() != expected:
            raise ValueError(f"Official InChI source changed: {relative}")
        if patch := patches["files"].get(relative):
            if patch["source_sha256"] != expected:
                raise ValueError(f"InChI patch source mismatch: {relative}")
            for change in patch["replacements"]:
                before, after = change["before"].encode(), change["after"].encode()
                if not before or change["count"] < 1 or data.count(before) != change["count"]:
                    raise ValueError(f"InChI patch context mismatch: {relative}")
                data = data.replace(before, after)
            if hashlib.sha256(data).hexdigest() != patch["patched_sha256"]:
                raise ValueError(f"InChI patched source mismatch: {relative}")
        files[relative] = data
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    destination = Path(tempfile.mkdtemp(prefix="kernel-source-", dir=output))
    try:
        for relative, data in files.items():
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
    except BaseException:
        shutil.rmtree(destination)
        raise
    return destination
