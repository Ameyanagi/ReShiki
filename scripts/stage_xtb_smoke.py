"""Stage the exact Windows ZIP executable for the independent Rust smoke harness."""

import argparse
import glob
import hashlib
import os
import tempfile
import zipfile
from pathlib import Path


def stage_application(archive: Path, directory: Path) -> tuple[Path, str]:
    with zipfile.ZipFile(archive) as stream:
        applications = [name for name in stream.namelist() if name.endswith("/reshiki.exe")]
        if len(applications) != 1:
            raise ValueError("Expected one application executable in the shipped ZIP")
        contents = stream.read(applications[0])
    expected = hashlib.sha256(contents).hexdigest()
    directory.mkdir(parents=True, exist_ok=True)
    binary = directory / "reshiki.exe"
    binary.write_bytes(contents)
    with binary.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != expected:
            raise ValueError("Staged application differs from the shipped ZIP entry")
    return binary.resolve(), expected


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", help="Glob selecting exactly one shipped Windows ZIP")
    args = parser.parse_args(argv)
    archives = [Path(name) for name in glob.glob(args.archive)]
    if len(archives) != 1:
        raise ValueError("Expected one shipped Windows ZIP matching the archive pattern")
    directory = Path(
        tempfile.mkdtemp(prefix="ReShiki xTB ZIP check ", dir=os.environ.get("RUNNER_TEMP"))
    )
    binary, digest = stage_application(archives[0], directory)
    if github_env := os.environ.get("GITHUB_ENV"):
        with Path(github_env).open("a", encoding="utf-8") as stream:
            stream.write(f"RESHIKI_TEST_PACKAGED_APP={binary}\n")
            stream.write(f"RESHIKI_TEST_PACKAGED_APP_SHA256={digest}\n")
    print(f"Shipped ZIP: {archives[0]}\nStaged executable: {binary}\nSHA256: {digest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
