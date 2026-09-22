"""Stage the pinned official InChI source for an explicit release build."""

import hashlib
import json
import shutil
import stat
import tempfile
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "tools/inchi-helper/source-manifest.json"
MAX_ARCHIVE_BYTES = 16 * 1024 * 1024
MAX_EXTRACTED_BYTES = 64 * 1024 * 1024
MAX_ENTRIES = 1024


def manifest():
    return json.loads(MANIFEST.read_text(encoding="utf-8"))


def verify_source(source, reference=None):
    reference = reference or manifest()
    source = Path(source).resolve(strict=True)
    for relative, expected in reference["files"].items():
        file = source / relative
        if not file.is_file():
            raise ValueError(f"Missing official InChI source: {relative}")
        with file.open("rb") as stream:
            actual = hashlib.file_digest(stream, "sha256").hexdigest()
        if actual != expected:
            raise ValueError(f"Official InChI source changed: {relative}")
    return source


def verify_archive(archive, reference):
    archive = Path(archive)
    if archive.stat().st_size > MAX_ARCHIVE_BYTES:
        raise ValueError("InChI source archive exceeds its byte limit")
    with archive.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    if digest != reference["archive_sha256"]:
        raise ValueError("Official InChI source archive checksum mismatch")


def extract_archive(archive, destination, reference):
    archive, destination = Path(archive), Path(destination)
    verify_archive(archive, reference)
    with zipfile.ZipFile(archive) as stream:
        entries = stream.infolist()
        if len(entries) > MAX_ENTRIES or sum(i.file_size for i in entries) > MAX_EXTRACTED_BYTES:
            raise ValueError("InChI source archive exceeds its extraction limits")
        seen = set()
        for entry in entries:
            name = entry.filename
            path = PurePosixPath(name)
            if (
                not path.parts
                or path.as_posix() != name.rstrip("/")
                or name != entry.orig_filename
                or path.parts[0] != reference["archive_root"]
                or path.is_absolute()
                or any(p in (".", "..") for p in name.split("/"))
                or "\\" in name
                or ":" in name
                or "\0" in name
                or name.rstrip("/").casefold() in seen
                or entry.flag_bits & 1
            ):
                raise ValueError(f"Invalid InChI archive entry: {name!r}")
            seen.add(name.rstrip("/").casefold())
            mode = stat.S_IFMT(entry.external_attr >> 16)
            if mode not in (0, stat.S_IFDIR if entry.is_dir() else stat.S_IFREG):
                raise ValueError(f"Unsupported InChI archive entry: {name!r}")
        # Validate every member before writing anything. The destination is a
        # newly created private staging directory, never an existing source tree.
        for entry in entries:
            target = destination.joinpath(*PurePosixPath(entry.filename).parts)
            if entry.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with stream.open(entry) as source, target.open("xb") as output:
                    shutil.copyfileobj(source, output)
    return verify_source(destination / reference["archive_root"], reference)


def download_archive(destination, reference):
    # The URL comes only from the checked-in manifest, never a CLI-supplied URL.
    url = reference["archive_url"]
    expected = (
        "https://github.com/IUPAC-InChI/InChI/releases/download/"
        f"v{reference['inchi_version']}/INCHI-1-SRC.zip"
    )
    if url != expected:
        raise ValueError("InChI archive URL is not the pinned official release")
    with (
        urllib.request.urlopen(url, timeout=60) as response,
        Path(destination).open("xb") as output,
    ):
        if not response.url.startswith("https://"):
            raise ValueError("InChI source download was redirected outside HTTPS")
        total = 0
        while data := response.read(64 * 1024):
            total += len(data)
            if total > MAX_ARCHIVE_BYTES:
                raise ValueError("InChI source download exceeds its byte limit")
            output.write(data)


def prepare_source(cache, *, source=None, archive=None, fetch=False):
    if sum((source is not None, archive is not None, fetch)) != 1:
        raise ValueError(
            "Choose exactly one audited InChI source, local archive, or explicit fetch"
        )
    reference = manifest()
    if source is not None:
        return verify_source(source, reference)
    if archive is not None:
        verify_archive(archive, reference)
    cache = Path(cache)
    cache.mkdir(parents=True, exist_ok=True)
    destination = cache / reference["archive_sha256"]
    if destination.exists():
        return verify_source(destination / reference["archive_root"], reference)
    with tempfile.TemporaryDirectory(prefix="inchi-source-", dir=cache) as temporary:
        stage = Path(temporary)
        if fetch:
            archive = stage / "official.zip"
            download_archive(archive, reference)
        extracted = stage / "extracted"
        extracted.mkdir()
        extract_archive(archive, extracted, reference)
        extracted.rename(destination)
    return verify_source(destination / reference["archive_root"], reference)
