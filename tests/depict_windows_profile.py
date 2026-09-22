"""Observe the pinned x64 reference runtime's CRT, including under ARM emulation."""

import argparse
import ctypes
import hashlib
import json
import struct
import sys
import sysconfig
from pathlib import Path

PHYSICAL_UCRT = "5c52e3a303baaac0e0af8bd9b96134993da34bc9d834a31ef37e1d2cdc7fe192"
SERVER2022_UCRT = "5e5ae0f8e4325ceb3ee767065dda6e03e980d401f00a2bdb8ca81ed90813920f"
EMULATED_UCRT = "529e795875178b906ea8758abc8de5c28339f336b3e7a121d7778ae040908de9"
PRIMITIVES = {
    "fma3": ("3ff29f25c111e687", "3fed63011ebd161e", "3fd954d5989f7ab4"),
    "no-fma3": ("3ff29f25c111e686", "3fed63011ebd161d", "3fd954d5989f7ab7"),
}
RINGS = {
    "fma3": ("3febb67ae8584cab", "bfdffffffffffffc", "bfebb67ae8584ca9", "bfe0000000000004"),
    "no-fma3": ("3febb67ae8584cab", "bfdffffffffffffc", "bfebb67ae8584ca8", "bfe0000000000004"),
}


def classify(module_sha256, observed, ring):
    """Identify an independently captured DLL and dispatch, without Rust output."""
    profiles = {
        (PHYSICAL_UCRT, PRIMITIVES["fma3"], RINGS["fma3"]): "fma3",
        (PHYSICAL_UCRT, PRIMITIVES["no-fma3"], RINGS["no-fma3"]): "no-fma3",
        (EMULATED_UCRT, PRIMITIVES["no-fma3"], RINGS["no-fma3"]): "no-fma3",
        (SERVER2022_UCRT, PRIMITIVES["no-fma3"], RINGS["fma3"]): "server2022",
    }
    return profiles.get((module_sha256, tuple(observed), tuple(ring)), "uncaptured")


def bits(value):
    return struct.pack(">d", value).hex()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fma3", choices=("0", "1"))
    parser.add_argument(
        "--observe", action="store_true", help="Record an unknown runtime for live replay"
    )
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
    phase = struct.unpack(">d", bytes.fromhex("4000c152382d7365"))[0]
    ring = tuple(
        bits(getattr(runtime, name)(x)) for x in (phase, 2.0 * phase) for name in ("sin", "cos")
    )
    module_name = ctypes.windll.kernel32.GetModuleFileNameW
    module_name.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_uint32]
    module_name.restype = ctypes.c_uint32
    buffer = ctypes.create_unicode_buffer(32768)
    length = module_name(runtime._handle, buffer, len(buffer))
    if not length or length >= len(buffer):
        raise SystemExit("Cannot identify the loaded reference UCRT")
    module = Path(buffer.value)
    module_sha256 = hashlib.sha256(module.read_bytes()).hexdigest()
    profile = classify(module_sha256, observed, ring)
    if profile == "uncaptured" and not args.observe:
        raise SystemExit(
            f"Uncaptured original Windows CRT: SHA256={module_sha256}, "
            f"acos={observed}, ring={ring}. "
            "Run scripts/setup_windows_depict_reference.py for exact live references, "
            "then preserve the independently validated capture before using saved fixtures."
        )
    print(
        json.dumps(
            dict(
                profile=profile,
                module=str(module),
                module_sha256=module_sha256,
                ring_bits=ring,
                primitive_bits=observed,
                python_platform=sysconfig.get_platform(),
                reference_version=rdBase.rdkitVersion,
                forced_fma3=args.fma3,
            )
        )
    )


if __name__ == "__main__":
    main()
