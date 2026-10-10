"""Regenerate the separately licensed, pinned offline NMR reference index."""

import argparse
import hashlib
import subprocess
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE_URL = "https://downloads.sourceforge.net/project/nmrshiftdb2/data/nmrshiftdb2withsignals.sd"
SOURCE_SHA256 = "0e86688360e23c88ccf0eb82a1251315fa57ec6f5b376dc8886f3311793a0afe"


def generate(source):
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    if digest != SOURCE_SHA256:
        raise SystemExit(f"Refusing changed NMR source: expected {SOURCE_SHA256}, got {digest}")
    subprocess.run(
        [
            "cargo",
            "run",
            "--locked",
            "-p",
            "reshiki-io",
            "--example",
            "nmr_index",
            "--",
            str(source),
            str(ROOT / "data/nmr/index.tsv"),
            str(ROOT / "data/nmr/validation.json"),
        ],
        cwd=ROOT,
        check=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source", type=Path, help="Already downloaded exact 2026-03-15 SDF export"
    )
    args = parser.parse_args()
    if args.source:
        generate(args.source.resolve())
    else:
        with tempfile.TemporaryDirectory(prefix="reshiki-nmr-source-") as temporary:
            source = Path(temporary) / "nmrshiftdb2withsignals.sd"
            print(f"Downloading pinned NMR export from {SOURCE_URL}")
            with (
                urllib.request.urlopen(SOURCE_URL, timeout=60) as response,
                source.open("wb") as output,
            ):
                while block := response.read(1024 * 1024):
                    output.write(block)
            generate(source)


if __name__ == "__main__":
    main()
