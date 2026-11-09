"""Reject wheels missing complete project, incorporated-source or dependency notices."""

import argparse
import glob
import sys
import zipfile
from email.parser import BytesParser
from pathlib import Path, PurePosixPath

from license_notices import PROJECT_FILES, THIRD_PARTY_FILE

ROOT = Path(__file__).resolve().parents[1]
REQUIRED_FILES = ("LICENSE-MIT", "LICENSE-APACHE", "NOTICE", THIRD_PARTY_FILE)
SOURCE_FILES = (
    "licenses/openclatura/LICENSE",
    "licenses/openclatura/NOTICE",
    "licenses/rdkit/LICENSE",
    "licenses/rdkit/NOTICE",
    "licenses/centres/LICENSE",
    "licenses/centres/NOTICE",
    "licenses/stdlib/llvm-LICENSE",
    "licenses/stdlib/msvc-LICENSE",
    "licenses/stdlib/NOTICE",
    "licenses/cpython/LICENSE",
    "licenses/cpython/NOTICE",
)
# Check the complete upstream grant beside its copyright, so another package's
# MIT text cannot conceal a truncated OPSIN license. Ignore only whitespace.
OPSIN_LICENSE = """Copyright 2017 Daniel Lowe

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
"""


def verify_wheel(wheel, root=ROOT):
    """Inspect the final archive without relying on Cargo or an installed package."""
    with zipfile.ZipFile(wheel) as archive:
        names = archive.namelist()
        metadata_paths = [
            name
            for name in names
            if len(PurePosixPath(name).parts) == 2
            and PurePosixPath(name).parts[0].endswith(".dist-info")
            and PurePosixPath(name).name == "METADATA"
        ]
        if len(metadata_paths) != 1:
            raise ValueError("Expected exactly one .dist-info/METADATA file")
        metadata_path = metadata_paths[0]
        prefix = f"{metadata_path.rsplit('/', 1)[0]}/licenses/"
        metadata = BytesParser().parsebytes(archive.read(metadata_path))
        declared = set()
        for value in metadata.get_all("License-File", []):
            relative = PurePosixPath(value.strip())
            if relative.is_absolute() or ".." in relative.parts or "\\" in value:
                raise ValueError(f"Invalid METADATA License-File path: {value}")
            member = prefix + relative.as_posix()
            if member not in names:
                raise ValueError(f"METADATA License-File does not exist: {value}")
            declared.add(member)
        payloads = {}
        for required in (*REQUIRED_FILES, "LICENSE"):
            matches = [
                name
                for name in names
                if name.startswith(prefix) and PurePosixPath(name).name == required
            ]
            if not matches and required == "LICENSE":
                continue
            if len(matches) != 1:
                raise ValueError(f"Expected exactly one {required} under .dist-info/licenses/")
            member = matches[0]
            if member not in declared:
                raise ValueError(f"Missing METADATA License-File declaration for {required}")
            payloads[required] = archive.read(member)
        for name in PROJECT_FILES:
            if name in payloads and payloads[name] != (root / name).read_bytes():
                raise ValueError(f"Project license file differs from repository: {name}")
        notices = payloads[THIRD_PARTY_FILE]
        for relative in SOURCE_FILES:
            if relative.encode() not in notices:
                raise ValueError(f"Missing source attribution in consolidated notices: {relative}")
            if (root / relative).read_bytes() not in notices:
                raise ValueError(
                    f"Missing complete source terms in consolidated notices: {relative}"
                )
        for package in ("nazuchem-n2s", "nazuchem-kernel"):
            if f"DEPENDENCY: {package}@".encode() not in notices:
                raise ValueError(f"Missing naming dependency notices: {package}")
        normalized = " ".join(notices.decode("utf-8").split())
        if " ".join(OPSIN_LICENSE.split()) not in normalized:
            raise ValueError("Missing complete OPSIN MIT license and Daniel Lowe copyright")
        if "https://github.com/dan2097/opsin" not in normalized:
            raise ValueError("Missing OPSIN upstream source attribution")


def wheel_paths(patterns):
    """Expand globs here as well as in shells that already expand them."""
    result = {}
    for pattern in patterns:
        matches = [
            Path(name)
            for name in glob.glob(str(pattern), recursive=True)
            if Path(name).is_file() and Path(name).suffix == ".whl"
        ]
        if not matches:
            raise ValueError(f"No wheel files matched: {pattern}")
        for match in sorted(matches):
            result[match.resolve()] = match
    return list(result.values())


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("wheels", nargs="+", help="Wheel paths or quoted glob patterns")
    arguments = parser.parse_args(argv)
    try:
        wheels = wheel_paths(arguments.wheels)
    except ValueError as error:
        parser.error(str(error))
    failed = False
    for wheel in wheels:
        try:
            verify_wheel(wheel)
        except (OSError, ValueError, zipfile.BadZipFile) as error:
            print(f"{wheel}: {error}", file=sys.stderr)
            failed = True
        else:
            print(f"Verified complete license notices: {wheel}")
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
