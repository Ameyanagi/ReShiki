"""Build a development bundle, or a portable bundle with --portable (requires uv)."""

import argparse
import platform

from build_release import ROOT, mac_bundle, run, runtime_project


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--portable",
        "--standalone",
        dest="portable",
        action="store_true",
        help="Include worker project; uv is required",
    )
    parser.add_argument("--release", action="store_true", help="Use an optimized Rust binary")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise SystemExit("Use scripts/build_release.py for Windows or Linux")
    profile = "release" if args.release else "debug"
    run(["cargo", "build", "--locked", *(["--release"] if args.release else [])], cwd=ROOT)
    destination = ROOT / ("dist/ReShiki.app" if args.portable else f"target/{profile}/ReShiki.app")
    print(mac_bundle(destination, profile, runtime_project() if args.portable else None))


if __name__ == "__main__":
    main()
