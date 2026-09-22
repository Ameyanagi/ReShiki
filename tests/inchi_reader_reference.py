"""Replay original native import captures, including exact input-text boundaries."""

import argparse
import gzip
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BOUNDARY = ROOT / "tests/fixtures/inchi-reader-text.json.gz"


def generate(args):
    # The original capture harness and exact InchiToMol body stay independent
    # of this helper. Only its line transport is changed to decode hex text,
    # allowing embedded NUL/newline bytes to reach the original native API.
    subprocess.run(
        [
            sys.executable,
            str(ROOT / "tests/build_inchi_output_oracle.py"),
            "--rdkit-source",
            str(args.rdkit_source),
            "--inchi-source",
            str(args.inchi_source),
        ],
        check=True,
    )
    manifest = json.loads((ROOT / "artifacts/inchi-output-build.json").read_text())
    original = ROOT / "tests/inchi_output_reference.cpp"
    source = original.read_text()
    needle = "in.get();std::getline(in,text);"
    assert source.count(needle) == 1
    source = source.replace(needle, needle + "text=observe::unhex(text);")
    generated = ROOT / "artifacts/inchi-reader-oracle.cpp"
    generated.write_text(source)
    oracle = ROOT / "artifacts/inchi-reader-oracle"
    command = [str(generated) if value == str(original) else value for value in manifest["command"]]
    command[command.index("-o") + 1] = str(oracle)
    subprocess.run(command, check=True)
    manifest["command"] = command
    manifest["sources"][str(generated)] = hashlib.sha256(generated.read_bytes()).hexdigest()
    (ROOT / "artifacts/inchi-reader-build.json").write_text(json.dumps(manifest, indent=2) + "\n")
    import rdkit

    libs = (
        Path(rdkit.__file__).parent / ".dylibs"
        if sys.platform == "darwin"
        else Path(rdkit.__file__).parent.parent / "rdkit.libs"
    )
    loader = "DYLD_LIBRARY_PATH" if sys.platform == "darwin" else "LD_LIBRARY_PATH"
    valid = "InChI=1S/CH4/h1H4"
    values = [
        "",
        " ",
        "\n",
        "invalid",
        "InChI=1S/",
        "\0" + valid,
        "invalid\0" + valid,
        "式" + valid,
        valid + "式",
    ]
    values += [
        valid + suffix
        for suffix in (
            "",
            " ",
            "\t",
            "\r",
            "\n",
            "\r\n",
            " junk",
            "\njunk",
            "\0junk",
            "\0InChI=invalid",
        )
    ]
    values += [prefix + valid for prefix in (" ", "\t", "\r", "\n")]
    values += [valid + " " * (2 * 1024 * 1024 - len(valid) + extra) for extra in (0, 1)]
    records = []
    for index, value in enumerate(values):
        result = subprocess.run(
            [str(oracle)],
            input="I 0 0 " + value.encode().hex() + "\n",
            text=True,
            capture_output=True,
            timeout=30,
            check=True,
            env={**os.environ, loader: str(libs)},
        )
        captured = json.loads(result.stdout)
        assert captured["raw"] is not None
        records.append(
            dict(
                name=f"text-boundary/{index}",
                inchi=value,
                raw=captured["raw"],
                limit=len(value.encode()) > 2 * 1024 * 1024,
            )
        )
    header = dict(
        rdkit_version="2026.03.6",
        inchi_version="1.07.3",
        adapter_sha256=manifest["sources"][str(args.rdkit_source / "External/INCHI-API/inchi.cpp")],
        capture_sha256=hashlib.sha256(original.read_bytes()).hexdigest(),
        transport_capture_sha256=hashlib.sha256(generated.read_bytes()).hexdigest(),
    )
    data = "".join(
        json.dumps(row, separators=(",", ":")) + "\n" for row in [header, *records]
    ).encode()
    BOUNDARY.write_bytes(gzip.compress(data, mtime=0))
    print(f"Captured {len(records)} original native text boundaries", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--generate", action="store_true")
    parser.add_argument("--boundaries", action="store_true")
    parser.add_argument("--rdkit-source", type=Path)
    parser.add_argument("--inchi-source", type=Path)
    args = parser.parse_args()
    if args.generate:
        generate(args)
        return
    if args.boundaries:
        with gzip.open(BOUNDARY, "rt") as source:
            for line in source:
                print(line, end="")
        return
    with gzip.open(ROOT / "tests/fixtures/inchi-output.jsonl.gz", "rt") as source:
        print(next(source), end="")
        for line in source:
            case = json.loads(line)
            if case["operation"] == "import":
                case["reconstruct"] = True
                print(json.dumps(case, separators=(",", ":")))


if __name__ == "__main__":
    main()
