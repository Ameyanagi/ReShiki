"""Build a development bundle, or a relocatable bundle with --standalone."""

import argparse
import platform

from build_release import ROOT, freeze_worker, mac_bundle, run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--standalone", action="store_true", help="Bundle Python and RDKit")
    parser.add_argument("--release", action="store_true", help="Use an optimized Rust binary")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise SystemExit("Use scripts/build_release.py for Windows or Linux")
    profile = "release" if args.release else "debug"
    run(["cargo", "build", "--locked", *(["--release"] if args.release else [])], cwd=ROOT)
    destination = ROOT / ("dist/Moruno.app" if args.standalone else f"target/{profile}/Moruno.app")
    print(mac_bundle(destination, profile, freeze_worker() if args.standalone else None))


if __name__ == "__main__":
    main()
