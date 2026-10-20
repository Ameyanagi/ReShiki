"""Render every harness .rsk with the repo CLI: reshiki --cli render (PNG + SVG)."""

import pathlib
import subprocess
import sys

binary, root = sys.argv[1], pathlib.Path(sys.argv[2])
for rsk in sorted(root.glob("*/*.rsk")):
    for ext, extra in (("png", ["--width", "600", "--height", "400"]), ("svg", [])):
        out = rsk.with_suffix("." + ext)
        subprocess.run(
            [binary, "--cli", "render", str(rsk), "-o", str(out), "--force", *extra],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        print(out)
