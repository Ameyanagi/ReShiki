"""Build a native development bundle, or a portable bundle with --portable."""

import argparse
import platform

from build_release import ROOT, host_target, mac_bundle, run


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
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise SystemExit("Use scripts/build_release.py for Windows or Linux")
    target = host_target()
    if target not in {"aarch64-apple-darwin", "x86_64-apple-darwin"}:
        raise SystemExit("ReShiki requires an Apple Silicon or Intel macOS Rust toolchain")
    profile = "release" if args.release else "debug"
    run(
        [
            "cargo",
            "build",
            "--locked",
            "--bin",
            "reshiki",
            *(["--release"] if args.release else []),
        ],
        cwd=ROOT,
    )
    destination = ROOT / ("dist/ReShiki.app" if args.portable else f"target/{profile}/ReShiki.app")
    print(
        mac_bundle(
            destination,
            profile,
        )
    )


if __name__ == "__main__":
    main()
