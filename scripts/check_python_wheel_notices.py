"""Verify wheels preserve the generated license files and declare them in METADATA."""

import glob
import sys
import zipfile
from email.parser import BytesParser
from pathlib import Path

LICENSES = Path(__file__).resolve().parents[1] / "crates/python/wheel-licenses"


def verify_wheel(wheel, licenses=LICENSES):
    expected = {
        path.relative_to(licenses).as_posix(): path.read_bytes()
        for path in licenses.rglob("*")
        if path.is_file()
    }
    with zipfile.ZipFile(wheel) as archive:
        metadata_path = next(
            name for name in archive.namelist() if name.endswith(".dist-info/METADATA")
        )
        metadata = BytesParser().parsebytes(archive.read(metadata_path))
        prefix = metadata_path.removesuffix("METADATA") + "licenses/wheel-licenses/"
        actual = {
            name.removeprefix(prefix)
            for name in archive.namelist()
            if name.startswith(prefix) and not name.endswith("/")
        }
        if actual != expected.keys():
            raise ValueError(
                f"License files differ: expected {sorted(expected)}, got {sorted(actual)}"
            )
        for name, content in expected.items():
            if f"wheel-licenses/{name}" not in metadata.get_all("License-File", []):
                raise ValueError(f"Missing License-File declaration: {name}")
            if archive.read(prefix + name) != content:
                raise ValueError(f"License file has differing bytes: {name}")


if __name__ == "__main__":
    if not sys.argv[1:]:
        sys.exit("Usage: check_python_wheel_notices.py WHEEL [WHEEL ...]")
    for pattern in sys.argv[1:]:
        wheels = glob.glob(pattern)
        if not wheels:
            sys.exit(f"No wheel files matched: {pattern}")
        for wheel in wheels:
            verify_wheel(wheel)
            print(f"Verified license notices: {wheel}")
