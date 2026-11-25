"""Verify wheels preserve the generated license files and declare them in METADATA."""

import argparse
import glob
import zipfile
from email.parser import BytesParser
from pathlib import Path

LICENSES = Path(__file__).resolve().parents[1] / "crates/python/wheel-licenses"
XTB_LICENSES = Path(__file__).resolve().parents[1] / "crates/python-xtb/wheel-licenses"
AXTB_ATTRIBUTIONS = tuple(
    f"licenses/axtb/{name}"
    for name in ("COPYING", "COPYING.LESSER", "NOTICE", "CHANGES-reshiki.md", "SOURCE")
)


def verify_wheel(wheel, licenses=None, *, xtb=False):
    licenses = (XTB_LICENSES if xtb else LICENSES) if licenses is None else licenses
    expected = {
        path.relative_to(licenses).as_posix(): path.read_bytes()
        for path in licenses.rglob("*")
        if path.is_file()
    }
    if "THIRD-PARTY-NOTICES.txt" not in expected:
        raise ValueError(
            f"No generated notices in {licenses}; run scripts/license_notices.py first"
        )
    attribution_lines = expected["THIRD-PARTY-NOTICES.txt"].splitlines()
    required = (
        AXTB_ATTRIBUTIONS
        if xtb
        else ("licenses/liberation-fonts/LICENSE", "licenses/liberation-fonts/NOTICE")
    )
    for name in required:
        if f"  {name}".encode() not in attribution_lines:
            raise ValueError(f"Missing source attribution: {name}")
    if not xtb and any(line.lstrip().startswith(b"licenses/axtb/") for line in attribution_lines):
        raise ValueError("The base reshiki wheel must not contain licenses/axtb/ notices")
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--xtb", action="store_true", help="Check a reshiki-xtb engine wheel")
    parser.add_argument("wheels", nargs="+", metavar="WHEEL")
    args = parser.parse_args()
    for pattern in args.wheels:
        wheels = glob.glob(pattern)
        if not wheels:
            parser.error(f"No wheel files matched: {pattern}")
        for wheel in wheels:
            verify_wheel(wheel, xtb=args.xtb)
            print(f"Verified license notices: {wheel}")


if __name__ == "__main__":
    main()
