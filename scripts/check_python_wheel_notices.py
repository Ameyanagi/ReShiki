"""Verify wheel license files, or a release engine wheel's version and source link."""

import argparse
import glob
import re
import zipfile
from email.parser import BytesParser
from pathlib import Path

LICENSES = Path(__file__).resolve().parents[1] / "crates/python/wheel-licenses"
XTB_LICENSES = Path(__file__).resolve().parents[1] / "crates/python-xtb/wheel-licenses"
AXTB_ATTRIBUTIONS = tuple(
    f"licenses/axtb/{name}"
    for name in ("COPYING", "COPYING.LESSER", "NOTICE", "CHANGES-reshiki.md", "SOURCE")
)
RELEASE_URL = "https://github.com/Ameyanagi/ReShiki/releases/tag/"


def release_version(tag):
    """Convert the stable and alpha/beta/rc SemVer release tags to PEP 440."""
    component = r"(?:0|[1-9][0-9]*)"
    match = re.fullmatch(
        rf"v({component}\.{component}\.{component})(?:-(alpha|beta|rc)\.?([0-9]+))?", tag
    )
    if match is None:
        raise ValueError(f"Unsupported Python release tag: {tag!r}")
    version, stage, number = match.groups()
    if stage:
        version += {"alpha": "a", "beta": "b", "rc": "rc"}[stage] + str(int(number))
    return version


def verify_release_wheel(wheel, tag):
    """Match both wheel versions and its concrete source-release links to the tag."""
    expected = release_version(tag)
    name = Path(wheel).name
    match = re.fullmatch(r"reshiki_xtb-([^-]+)-.+\.whl", name)
    if match is None or match[1] != expected:
        raise ValueError(f"Wheel {name} must have release version {expected}")
    with zipfile.ZipFile(wheel) as archive:
        paths = [name for name in archive.namelist() if name.endswith(".dist-info/METADATA")]
        if len(paths) != 1:
            raise ValueError(f"Expected one wheel METADATA file in {name}")
        content = archive.read(paths[0])
    metadata = BytesParser().parsebytes(content)
    if metadata.get("Name", "").replace("_", "-").lower() != "reshiki-xtb":
        raise ValueError(f"Expected reshiki-xtb metadata in {name}")
    if metadata.get_all("Version") != [expected]:
        raise ValueError(f"Wheel metadata must have release version {expected}: {name}")
    links = re.findall(re.escape(RELEASE_URL) + r"([^\s)>\]`]+)", content.decode("utf-8"))
    # The README also shows a literal v<version> template, which is not a release link.
    tags = {link for link in links if re.match(r"v[0-9]", link)}
    if tags != {tag}:
        raise ValueError(f"Wheel source links must point to {RELEASE_URL}{tag}: {name}")


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
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--xtb", action="store_true", help="Check engine wheel license notices")
    mode.add_argument(
        "--release-tag", help="Check only engine wheel versions and source links for this tag"
    )
    parser.add_argument("wheels", nargs="+", metavar="WHEEL")
    args = parser.parse_args()
    for pattern in args.wheels:
        wheels = glob.glob(pattern)
        if not wheels:
            parser.error(f"No wheel files matched: {pattern}")
        for wheel in wheels:
            if args.release_tag:
                verify_release_wheel(wheel, args.release_tag)
                print(f"Verified release version and source link: {wheel}")
            else:
                verify_wheel(wheel, xtb=args.xtb)
                print(f"Verified license notices: {wheel}")


if __name__ == "__main__":
    main()
