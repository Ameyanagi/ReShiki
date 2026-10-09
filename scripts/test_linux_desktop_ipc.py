#!/usr/bin/env python3
"""Run the exact Linux IPC module in isolation from GUI/clipboard dependencies."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    if sys.platform != "linux":
        raise SystemExit("Native Linux runtime is required; cross-checking is not runtime evidence")
    sources = [ROOT / "native/linux/src/desktop.rs", ROOT / "native/linux/src/desktop/tests.rs"]
    receipt = {
        str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in sources
    }
    print(
        json.dumps({"source_sha256": receipt, "platform": sys.platform}, sort_keys=True), flush=True
    )
    with tempfile.TemporaryDirectory(prefix="reshiki-desktop-ipc-") as temporary:
        package = Path(temporary)
        (package / "src/desktop").mkdir(parents=True)
        for source, destination in zip(
            sources, [package / "src/desktop.rs", package / "src/desktop/tests.rs"], strict=True
        ):
            shutil.copyfile(source, destination)
        (package / "src/lib.rs").write_text("#![forbid(unsafe_code)]\npub mod desktop;\n")
        (package / "Cargo.toml").write_text(
            '[package]\nname="reshiki-desktop-ipc-linux-check"\nversion="0.1.0"\nedition="2024"\n'
            '[dependencies]\nrustix={version="=1.1.4",features=["fs","net","process"]}\n'
        )
        environment = os.environ.copy()
        environment.update(
            CARGO_BUILD_JOBS="4",
            CARGO_INCREMENTAL="0",
            CARGO_PROFILE_DEV_DEBUG="0",
            CARGO_PROFILE_TEST_DEBUG="0",
            CARGO_TARGET_DIR=str(package / "target"),
        )
        cargo = ["cargo", "+1.99.0", "--manifest-path", str(package / "Cargo.toml")]
        # Cargo options follow the subcommand. The generated isolated lockfile
        # is printed so the small dependency boundary is reviewable in CI.
        subprocess.run(
            [cargo[0], cargo[1], "generate-lockfile", *cargo[2:]], env=environment, check=True
        )
        print((package / "Cargo.lock").read_text(), flush=True)
        subprocess.run(
            [
                cargo[0],
                cargo[1],
                "clippy",
                *cargo[2:],
                "--locked",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
            env=environment,
            check=True,
        )
        subprocess.run(
            [
                cargo[0],
                cargo[1],
                "test",
                *cargo[2:],
                "--locked",
                "--",
                "--nocapture",
                "--test-threads=1",
            ],
            env=environment,
            check=True,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
