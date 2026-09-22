"""Direct InChI 1.07.3 key/status oracle; no reimplementation of its algorithm."""

import argparse
import ctypes
import hashlib
import io
import json
import locale
import os
import pickle
import random
from pathlib import Path

import rdkit
from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdinchi

INCHI_VERSION = "1.07.3"
RDKIT_VERSION = "2026.03.6"
HARD_SET_SHA256 = "603b0a620db6aa664c399f616b162a75ecd91cc56994099aab0c8bbaee2d4a28"


def native_key():
    """Use the C entry point so failed inputs retain their numeric status."""
    package = Path(rdkit.__file__).parent
    roots = (package / ".dylibs", package.parent / "rdkit.libs", package)
    candidates = sorted({path for root in roots if root.is_dir() for path in root.rglob("*Inchi*")})
    for path in candidates:
        if path.suffix.lower() not in (".so", ".dylib", ".dll") and ".so." not in path.name:
            continue
        try:
            library = ctypes.CDLL(str(path))
            function = library.GetINCHIKeyFromINCHI
        except (OSError, AttributeError):
            continue
        function.argtypes = [
            ctypes.c_char_p,
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_char_p,
            ctypes.c_char_p,
            ctypes.c_char_p,
        ]
        function.restype = ctypes.c_int

        def evaluate(text):
            buffer = ctypes.create_string_buffer(29)
            status = function(text.encode("utf-8"), 0, 0, buffer, None, None)
            key = buffer.value.decode("ascii") if status == 0 else None
            public_key = rdinchi.InchiToInchiKey(text)
            assert public_key == (key or ""), (text, status, key, public_key)
            return status, key

        return evaluate
    raise RuntimeError("Cannot find the installed InChI C key entry point")


class StringDictionary(pickle.Unpickler):
    """The pinned historical fixture is a protocol-0 dictionary of strings."""

    def find_class(self, module, name):
        raise pickle.UnpicklingError(f"Unexpected global in pinned fixture: {module}.{name}")


def hard_set(explicit):
    root = Path(__file__).resolve().parents[1]
    candidates = [
        explicit or os.environ.get("RESHIKI_INCHI_HARD_SET"),
        root / "artifacts/pubchem-hard-set.inchi",
        root.parent / "rdkit/rdkit/Chem/test_data/pubchem-hard-set.inchi",
    ]
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            raw = Path(candidate).read_bytes()
            # Accept checkout newline conversion, while pinning the content.
            raw = raw.replace(b"\r\n", b"\n")
            assert hashlib.sha256(raw).hexdigest() == HARD_SET_SHA256
            values = StringDictionary(io.BytesIO(raw), encoding="latin1").load()
            assert isinstance(values, dict) and len(values) == 1181
            assert all(isinstance(k, str) and isinstance(v, str) for k, v in values.items())
            return values
    if os.environ.get("RESHIKI_REQUIRE_INCHI_HARD_SET") == "1":
        raise RuntimeError("Required pinned PubChem InChI hard set is unavailable")
    return {}


def inputs(corpus):
    methane = "InChI=1S/CH4/h1H4"
    for name, prefix, fill in (
        ("major", methane, "C"),
        ("minor", methane + "/t", "1"),
        ("ignored", methane + "\0", "x"),
    ):
        yield f"input-boundary/{name}", prefix + fill * (1_048_576 - len(prefix))
    for index, text in enumerate(
        [
            "",
            "InChI=",
            "InChI=1",
            "InChI=1/",
            "InChI=1S/",
            "InChI=1B/",
            "InChI=1//",
            "InChI=1S//",
            "InChI=1B/?",
            "InChI=1S/?",
            "InChI=1S/C",
            "InChI=1S/C/",
            "InChI=1S/C/p",
            "InChI=1S/C/p/t",
            "InChI=1S/C/p+0",
            "InChI=1S/C/p+1/p-2",
            "InChI=1S/C/p+1/q+2",
            "InChI=1S/C/t1/f/h1H",
            "InChI=1S/C/f/h1H",
            "InChI=1S/C/rC",
            "InChI=1/C/f/h1H",
            "InChI=1B/C/rC",
            "InChI=1S/C/p+1/t1/p-2",
            "InChI=1S/C/h1H/p-12/t1-/m0/s1",
        ]
    ):
        yield f"special/{index}", text
    for position in range(len(methane) + 1):
        yield f"truncated/{position}", methane[:position]
        for value in range(128):
            char = chr(value)
            yield f"ascii/{position}/{value}", methane[:position] + char + methane[position:]
    for prefix in ("InChI=1/", "InChI=1S/", "InChI=1B/"):
        for suffix in ("", "\0junk", "\tname", "\nname", " label", "é", "日本語", "[x]", ":"):
            yield f"suffix/{prefix}/{suffix}", prefix + "CH4/h1H4" + suffix
        for char in "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ/?@*().é\0":
            yield f"first/{prefix}/{char}", prefix + char
        for code in (
            *range(0x80, 0x800, 64),
            *range(0x800, 0xD800, 4096),
            *range(0xE000, 0x10000, 4096),
            *range(0x10000, 0x110000, 0x40000),
            0x100000,
        ):
            yield f"unicode-first/{prefix}/{code}", prefix + chr(code)
        for size in (0, 1, 2, 251, 252, 253, 254, 255, 256, 257, 1024, 65536):
            yield f"minor-boundary/{prefix}/{size}", prefix + "CH4/t" + "1" * size
    numbers = [str(n) for n in range(-32, 33)]
    numbers += ["", "+", "-", "00", "+00", "-00", "x", "--1", "++1", "1.5", "01x"]
    for power in (31, 32, 63, 64, 127):
        for delta in (-2, -1, 0, 1, 2):
            for sign in ("", "-", "+"):
                numbers.append(sign + str(2**power + delta))
    numbers += ["9" * 1024, "-" + "9" * 1024]
    for index, number in enumerate(numbers):
        for tail in ("", "/t1-", "/q+2", "/p-1", "/f/h1H", " junk"):
            yield f"proton/{index}/{tail}", methane + "/p" + number + tail
    for index, smiles in enumerate(
        [
            "CCO",
            "c1ccccc1",
            "[Na+].[Cl-]",
            "[CH3]",
            "[2H]O[3H]",
            "[13CH3][18OH]",
            "N[C@@H](C)C(=O)O",
            "N[C@H](C)C(=O)O",
            "F/C=C/F",
            "F/C=C\\F",
            "CC(=O)C",
            "CC(O)=C",
            "C1=CC=CN1",
            "[Fe+2].[Cl-].[Cl-]",
            "OCl(=O)(=O)=O",
            "C[S@](=O)CC",
            "F[C@](Cl)(Br)I",
            "C1CC=CCCCCCC1",
            "*",
            "",
        ]
    ):
        mol = Chem.MolFromSmiles(smiles)
        assert mol is not None
        for option in ("", "/FixedH", "/RecMet", "/SUU", "/FixedH /RecMet"):
            text, status, _, _, _ = rdinchi.MolToInchi(mol, option)
            if status in (0, 1):
                yield f"molecule/{index}/{option}", text
    rng = random.Random(10703)
    alphabet = "CHON0123456789+-(),.;=?@/chqptbmfsri"
    for index in range(40000):
        prefix = rng.choice(("InChI=1S/", "InChI=1/", "InChI=1B/"))
        body = "C" + "".join(rng.choices(alphabet, k=rng.randrange(1, 160)))
        yield f"generated/{index}", prefix + body
    for name, text in sorted(corpus.items()):
        yield f"hard-set/{name}", text
        yield f"hard-set-nonstandard/{name}", text.replace("InChI=1S/", "InChI=1/", 1)
        yield f"hard-set-experimental/{name}", text.replace("InChI=1S/", "InChI=1B/", 1)
        yield f"hard-set-tail/{name}", text + "\tPubChem CID=" + name


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hard-set", type=Path)
    args = parser.parse_args()
    assert rdBase.rdkitVersion == RDKIT_VERSION
    assert rdinchi.GetInchiVersion() == INCHI_VERSION
    RDLogger.DisableLog("rdApp.*")
    evaluate = native_key()
    corpus = hard_set(args.hard_set)
    system_locale = locale.setlocale(locale.LC_CTYPE)
    print(
        json.dumps(
            {
                "rdkit_version": rdBase.rdkitVersion,
                "inchi_version": INCHI_VERSION,
                "c_long_bytes": ctypes.sizeof(ctypes.c_long),
                "hard_set_count": len(corpus),
                "system_locale": system_locale,
            }
        )
    )
    # The C API's initial isalnum check depends on LC_CTYPE, while its later
    # alphabet scan is ASCII-only. Compare under the C locale and independently
    # audit the original process locale; never hide a change
    # to a generated chemical identifier behind locale normalization.
    for original_locale in (False, True):
        locale.setlocale(locale.LC_CTYPE, system_locale if original_locale else "C")
        for name, text in inputs(corpus):
            status, key = evaluate(text)
            print(
                json.dumps(
                    {
                        "name": name,
                        "input": text,
                        "status": status,
                        "key": key,
                        "original_locale": original_locale,
                    }
                )
            )


if __name__ == "__main__":
    main()
