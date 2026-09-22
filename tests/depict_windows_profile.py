"""Observe the pinned x64 reference runtime's CRT, including under ARM emulation."""

import argparse
import ctypes
import json
import struct
import sys
import sysconfig


def bits(value):
    return struct.pack(">d", value).hex()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fma3", choices=("0", "1"))
    args = parser.parse_args()
    if sys.platform != "win32" or sysconfig.get_platform() != "win-amd64":
        raise SystemExit("Use the pinned x64 Windows reference interpreter")
    from rdkit import rdBase

    if rdBase.rdkitVersion != "2026.03.6":
        raise SystemExit("Wrong native reference version")
    runtime = ctypes.CDLL("ucrtbase.dll")
    if args.fma3 is not None:
        switch = runtime._set_FMA3_enable
        switch.argtypes = [ctypes.c_int]
        switch.restype = ctypes.c_int
        requested = int(args.fma3)
        if switch(requested) != requested:
            raise SystemExit("Requested native CRT profile is unavailable")
    for name in ("acos", "sin", "cos"):
        function = getattr(runtime, name)
        function.argtypes = [ctypes.c_double]
        function.restype = ctypes.c_double
    ratio = struct.unpack(">d", bytes.fromhex("3fd954d5989f7ab5"))[0]
    angle = runtime.acos(ratio)
    observed = (bits(angle), bits(runtime.sin(angle)), bits(runtime.cos(angle)))
    profiles = {
        ("3ff29f25c111e687", "3fed63011ebd161e", "3fd954d5989f7ab4"): "fma3",
        ("3ff29f25c111e686", "3fed63011ebd161d", "3fd954d5989f7ab7"): "no-fma3",
    }
    if observed not in profiles:
        raise SystemExit(f"Uncaptured original Windows CRT profile: {observed}")
    print(
        json.dumps(
            dict(
                profile=profiles[observed],
                primitive_bits=observed,
                python_platform=sysconfig.get_platform(),
                reference_version=rdBase.rdkitVersion,
                forced_fma3=args.fma3,
            )
        )
    )


if __name__ == "__main__":
    main()
