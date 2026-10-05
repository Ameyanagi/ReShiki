"""Capture the x64 CRT's non-FMA math, independently of the Rust implementation."""

import ctypes
import hashlib
import json
import math
import os
import platform
import random
import struct
import sys
from pathlib import Path


def bits(value):
    return struct.pack(">d", value).hex()


def main():
    if sys.platform != "win32" or platform.machine().lower() not in ("amd64", "x86_64"):
        raise RuntimeError("Capture with the pinned x64 CPython on Windows")
    crt = ctypes.CDLL("ucrtbase")
    # This changes only the capturing process. It selects the same documented
    # algorithm as an x64 worker without FMA3, including Windows ARM emulation.
    previous = crt._get_FMA3_enable()
    try:
        if crt._set_FMA3_enable(0) != 0:
            raise RuntimeError("Could not select the non-FMA CRT implementation")
        values = [i * math.tau / 10_000 for i in range(-10_000, 10_001)]
        rng = random.Random(90210)
        values.extend(rng.uniform(-math.tau, math.tau) for _ in range(20_000))
        for boundary in [0.0, 2**-27, 2**-13, *(i * math.pi / 4 for i in range(1, 9))]:
            for direction in [-math.inf, math.inf]:
                value = boundary
                for _ in range(65):
                    if abs(value) <= math.tau:
                        values.extend([value, -value])
                    value = math.nextafter(value, direction)
        values.append(float.fromhex("0x1.4978f9e59d213p-1"))
        # Exact deduplication retains signed zero and subnormals.
        unique = {bits(value): value for value in values}
        library = Path(os.environ["SystemRoot"]) / "System32" / "ucrtbase.dll"
        header = dict(
            python=sys.version,
            platform=platform.platform(),
            ucrt_sha256=hashlib.sha256(library.read_bytes()).hexdigest(),
            fma3=False,
            cases=len(unique),
        )
        output = Path(sys.argv[1])
        with output.open("wb") as destination:
            for key, value in unique.items():
                destination.write(
                    bytes.fromhex(key + bits(math.sin(value)) + bits(math.cos(value)))
                )
        output.with_suffix(".json").write_text(json.dumps(header, indent=2) + "\n")
        print(json.dumps(header))
    finally:
        crt._set_FMA3_enable(previous)


if __name__ == "__main__":
    main()
