#!/usr/bin/env python3
"""Temporary negative controls for the finite evidence checker; no app runs."""

import copy
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def read(path):
    return json.loads(path.read_bytes())


def write(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def fingerprint(root):
    return {
        path.relative_to(root).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in root.rglob("*")
        if path.is_file()
    }


def descriptor(root, manifest, name):
    path = root / name
    entry = next(entry for entry in manifest["files"] if entry["path"] == name)
    entry["bytes"] = path.stat().st_size
    entry["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
    return entry


def mutate(root, case):
    manifest = read(root / "manifest.json")
    original = copy.deepcopy(manifest)
    restored = manifest["native_restore"]
    edited_path = restored
    if case == "raw-jpeg-byte":
        name = next(entry["path"] for entry in manifest["files"] if entry["role"] == "raw-jpeg")
        data = bytearray((root / name).read_bytes())
        data[-1] ^= 1
        (root / name).write_bytes(data)
        return "byte-hash"
    if case == "executable-correlation":
        name = manifest["correlation"]["native"]
        native = read(root / name)
        native["executable_sha256"] = "0" * 64
        write(root / name, native)
        descriptor(root, manifest, name)
    elif case == "path-escape":
        manifest["files"][0]["path"] = "../manifest.json"
    else:
        if case == "fixed-stretch-component":
            edited_path = manifest["native_stretch"]
        if case == "reopened-byte-identity":
            edited_path = manifest["native_reopened"]
            path = root / edited_path
            path.write_bytes(path.read_bytes() + b" ")
        else:
            document = read(root / edited_path)
            arrow = document["arrows"][0]
            if case == "anchor-gap":
                arrow["start_anchor"]["gap"] += 1.0
            elif case == "anchor-direction":
                arrow["start_anchor"]["direction"]["x"] += 0.1
            elif case == "anchor-target":
                arrow["start_anchor"]["target"]["mark"] = 2
            elif case == "cubic-control":
                arrow["cubic"][0]["y"] += 0.5
            elif case == "cage-depth":
                next(atom for atom in document["atoms"] if atom["id"] == 1001)["depth"] += 1.0
            elif case == "NMR-persistence":
                document["nmr_report"] = {"injected_control": True}
            elif case == "fixed-stretch-component":
                next(atom for atom in document["atoms"] if atom["id"] == 3001)["position"]["x"] += (
                    1.0
                )
            else:
                raise ValueError(case)
            write(root / edited_path, document)
        entry = descriptor(root, manifest, edited_path)
        name = manifest["correlation"]["native"]
        native = read(root / name)
        key = {
            restored: "native_restore_sha256",
            manifest["native_stretch"]: "native_stretch_sha256",
            manifest["native_reopened"]: "native_reopened_sha256",
        }[edited_path]
        native[key] = entry["sha256"]
        write(root / name, native)
        descriptor(root, manifest, name)
    assert manifest != original
    write(root / "manifest.json", manifest)
    return "semantic-or-correlation-after-updated-hashes"


def run():
    before = fingerprint(ROOT)
    command = [
        sys.executable,
        "-B",
        str(ROOT / "verify_evidence.py"),
        "--manifest",
        "manifest.json",
    ]
    baseline = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=15)
    assert baseline.returncode == 0, baseline.stderr
    results = []
    for case in (
        "raw-jpeg-byte",
        "anchor-gap",
        "anchor-direction",
        "anchor-target",
        "cubic-control",
        "cage-depth",
        "NMR-persistence",
        "fixed-stretch-component",
        "reopened-byte-identity",
        "executable-correlation",
        "path-escape",
    ):
        with tempfile.TemporaryDirectory(prefix="drawing-evidence-control-") as directory:
            root = Path(directory) / "packet"
            shutil.copytree(ROOT, root)
            gate = mutate(root, case)
            result = subprocess.run(
                [
                    sys.executable,
                    "-B",
                    str(root / "verify_evidence.py"),
                    "--manifest",
                    "manifest.json",
                ],
                cwd=root,
                capture_output=True,
                text=True,
                timeout=15,
            )
            assert result.returncode != 0 and "Evidence check failed:" in result.stderr, (
                case,
                result,
            )
            results.append(
                {
                    "case": case,
                    "expected_rejection": True,
                    "exit_code": result.returncode,
                    "gate": gate,
                    "stderr": result.stderr.strip(),
                }
            )
    assert before == fingerprint(ROOT), "Frozen originals were changed"
    print(
        json.dumps(
            {
                "status": "PRIVATE_STATIC_NEGATIVE_CONTROLS_COMPLETED",
                "runtime_or_gui_validation": False,
                "baseline_exit_code": baseline.returncode,
                "rejected_controls": results,
                "original_files_unchanged": len(before),
            },
            indent=2,
            allow_nan=False,
        )
    )


if __name__ == "__main__":
    if sys.flags.optimize:
        raise SystemExit("Run without -O: this checker requires assertions.")
    run()
