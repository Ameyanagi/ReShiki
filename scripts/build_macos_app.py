"""Build a native development bundle, or a portable bundle with --portable."""

import argparse
import platform
from pathlib import Path

from build_release import ROOT, mac_bundle, prepare_inchi_helper, run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--portable",
        "--standalone",
        dest="portable",
        action="store_true",
        help="Write the portable bundle to dist/ReShiki.app",
    )
    parser.add_argument("--release", action="store_true", help="Use an optimized Rust binary")
    parser.add_argument(
        "--inchi-helper",
        type=Path,
        default=ROOT / "artifacts/inchi-helper/reshiki-inchi-helper",
        help="Already built ARM64 native helper with matching build.json",
    )
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise SystemExit("Use scripts/build_release.py for Windows or Linux")
    if platform.machine() != "arm64":
        raise SystemExit("ReShiki supports Apple Silicon Macs")
    if not args.inchi_helper.is_file():
        raise SystemExit(
            "Build the native helper first: uv run --locked python "
            "scripts/build_inchi_helper.py --fetch-source"
        )
    helper, _ = prepare_inchi_helper("aarch64-apple-darwin", prebuilt=args.inchi_helper)
    profile = "release" if args.release else "debug"
    run(["cargo", "build", "--locked", *(["--release"] if args.release else [])], cwd=ROOT)
    destination = ROOT / ("dist/ReShiki.app" if args.portable else f"target/{profile}/ReShiki.app")
    print(
        mac_bundle(
            destination,
            profile,
            inchi_helper=helper,
        )
    )


if __name__ == "__main__":
    main()
