"""Floating-point values read through RDKit's public CX coordinate parser."""

import json
import math
import random
import struct

from rdkit import Chem, RDLogger, rdBase


def cases():
    yield from (
        "0",
        "-0",
        "+0",
        "nan",
        "-NaN",
        "inf",
        "-infinity",
        "1e309",
        "1e-9999",
        "1e-308",
        "5e-324",
        "0e99999",
        " 1",
        "1 ",
        "bad",
        "0x1p-1075",
        "0x1.1p-1075",
        "0x1.8p-1074",
        "0x1.fffffffffffff8p1023",
        "0x0.fffffffffffff8p-1022",
    )
    rng = random.Random(147284)
    for _ in range(4000):
        value = struct.unpack("!d", rng.getrandbits(64).to_bytes(8, "big"))[0]
        if math.isfinite(value):
            yield repr(value)
            yield value.hex()
    for exponent in (-1075, -1074, -1023, -1022, -1, 0, 1, 1022, 1023, 1024):
        for fraction in (
            "0",
            "00000000000008",
            "0000000000000800001",
            "00000000000018",
            "123456789abcdef01234",
            "ffffffffffffe7",
            "ffffffffffffe8",
            "fffffffffffff7",
            "fffffffffffff8",
            "ffffffffffffffff",
        ):
            for sign in ("", "-"):
                yield f"{sign}0x1.{fraction}p{exponent}"


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    params = Chem.SmilesParserParams()
    params.sanitize = False
    params.removeHs = False
    for text in cases():
        try:
            mol = Chem.MolFromSmiles(f"C |({text},,)|", params)
            if mol is None:
                raise ValueError("Rejected coordinate")
            value = mol.GetConformer().GetAtomPosition(0).x
            expected = (
                "nan"
                if math.isnan(value)
                else str(struct.unpack("!Q", struct.pack("!d", value))[0])
            )
        except (ValueError, RuntimeError, OverflowError):
            expected = None
        print(json.dumps(dict(text=text, expected=expected)))


if __name__ == "__main__":
    main()
