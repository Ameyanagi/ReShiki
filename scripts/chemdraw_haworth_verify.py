#!/usr/bin/env python3
"""Capture clipboard formats, RXN containers, and actual ChemDraw reopens."""

import argparse
import json
import subprocess
import time
import xml.etree.ElementTree as ET
from pathlib import Path

from chemdraw_haworth_corpus import ROOT, apple, quote

COPY_FORMATS = [
    "SMILES",
    "SLN",
    "InChI",
    "InChI Key",
    "CDXML Text",
    "MOL Text",
    "MOL V2000 Text",
    "3MF (Ball and Stick)",
    "3MF (Stick)",
    "PNG",
]


def select():
    apple(
        'tell application "ChemDraw" to activate\n'
        'tell application "System Events" to tell process "ChemDraw" '
        'to keystroke "a" using command down'
    )


def opened(path, notices=None):
    previous = set()
    if path.suffix == ".cds":
        previous = set(
            apple(
                "set AppleScript's text item delimiters to linefeed\n"
                'tell application "ChemDraw" to return (name of every document) as text'
            ).splitlines()
        )
    apple(
        f'tell application "ChemDraw" to open POSIX file {quote(path)}',
        alert_contains=path.name,
        notices=notices,
    )
    name = apple('tell application "ChemDraw" to get name of document 1')
    stationery = (
        path.suffix == ".cds" and name.startswith("Untitled stationery-") and name not in previous
    )
    if name != path.name and not stationery:
        raise RuntimeError(f"ChemDraw did not open {path.name}; front document is {name}")
    return name


def close(name):
    apple(f'tell application "ChemDraw" to close document {quote(name)} saving no')


def measure(name):
    select()
    d = f"document {quote(name)}"
    return {
        "atoms": int(apple(f'tell application "ChemDraw" to count atoms of {d}')),
        "bonds": int(apple(f'tell application "ChemDraw" to count bonds of {d}')),
        "smiles": apple(f'tell application "ChemDraw" to get SMILES of selection of {d}'),
    }


def reactions(directory, record):
    root = ET.parse(directory / "input.cdxml").getroot()
    page = root.find("page")
    if page is None:
        raise ValueError("Missing seed page")
    ET.SubElement(
        page,
        "arrow",
        id="9500",
        ArrowheadHead="Full",
        ArrowheadType="Solid",
        Tail3D="235 150 0",
        Head3D="295 150 0",
    )
    seed = directory / "reaction-container.cdxml"
    seed.write_text(ET.tostring(root, encoding="unicode", xml_declaration=True))
    name = opened(seed)
    record["reaction_exports"] = []
    try:
        for fmt, title in [
            ("rxn", "Reaction Molfile (RXN)"),
            ("rxn-v2000", "Reaction Molfile V2000 (RXN)"),
        ]:
            path = directory / (fmt + "-container.rxn")
            entry = {
                "format": fmt,
                "path": str(path.relative_to(directory.parent)),
                "context": "Single-reactant container; no transformation or product asserted",
            }
            record["reaction_exports"].append(entry)
            try:
                apple(
                    f'tell application "ChemDraw" to save document {quote(name)} '
                    f"in POSIX file {quote(path)} as {quote(title)}",
                    alert_document=name,
                )
                entry.update(status="saved", bytes=path.stat().st_size)
            except (OSError, RuntimeError) as error:
                entry.update(status="failed", error=str(error))
            print(record["id"], fmt, entry["status"], flush=True)
    finally:
        close(name)


def clipboard(directory, record, helper):
    name = opened(directory / "cdxml.cdxml")
    record["clipboard"] = []
    try:
        select()
        for title in COPY_FORMATS:
            entry = {"format": title}
            record["clipboard"].append(entry)
            menu = (
                f'menu item {quote(title)} of menu 1 of menu item "Copy As" '
                'of menu "Edit" of menu bar item "Edit" of menu bar 1'
            )
            prefix = 'tell application "System Events" to tell process "ChemDraw" to '
            if apple(prefix + "get enabled of " + menu) != "true":
                entry["status"] = "disabled"
                print(record["id"], title, "disabled", flush=True)
                continue
            before = subprocess.check_output([str(helper), "--change-count"], text=True)
            apple(
                'tell application "ChemDraw" to activate\n' + prefix + "click " + menu,
                alert_document=name,
            )
            changed = False
            for _ in range(100):
                after = subprocess.check_output([str(helper), "--change-count"], text=True)
                if after != before:
                    changed = True
                    break
                time.sleep(0.1)
            if not changed:
                entry["status"] = "clipboard-unchanged"
                print(record["id"], title, entry["status"], flush=True)
                continue
            folder = directory / "clipboard" / title.lower().replace(" ", "-")
            data = json.loads(subprocess.check_output([str(helper), str(folder)], text=True))
            entry.update(
                status="captured", path=str(folder.relative_to(directory.parent)), types=data
            )
            print(record["id"], title, "captured", flush=True)
    finally:
        close(name)


def reopen(output, record):
    for entry in record["exports"] + record.get("reaction_exports", []):
        if entry["status"] != "saved":
            continue
        path = output / entry["path"]
        try:
            notices = []
            name = opened(path, notices)
            try:
                entry["reopen"] = dict(status="opened", notices=notices, **measure(name))
            finally:
                close(name)
        except (OSError, RuntimeError) as error:
            entry["reopen"] = {
                "status": "not-imported" if entry["format"] == "svg" else "failed",
                "error": str(error),
            }
        print(record["id"], entry["format"], entry["reopen"], flush=True)


def reshiki(directory, record):
    record["reshiki_exports"] = []
    for format in ["cdxml", "cdx"]:
        path = directory / "reshiki" / ("reshiki." + format)
        entry = {"format": format, "path": str(path.relative_to(directory.parent))}
        record["reshiki_exports"].append(entry)
        name = opened(path)
        try:
            entry["reopen"] = dict(status="opened", **measure(name))
            # Ask ChemDraw to save its own interpreted document as additional
            # evidence; this is not a copy of ReShiki's serialized input.
            saved = directory / "reshiki" / ("chemdraw-resaved-" + format + ".cdxml")
            apple(
                f'tell application "ChemDraw" to save document {quote(name)} '
                f'in POSIX file {quote(saved)} as "ChemDraw XML"',
                alert_document=name,
            )
        finally:
            close(name)
        print(record["id"], format, entry["reopen"], flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["reactions", "clipboard", "reopen", "reshiki"])
    parser.add_argument(
        "--output", type=Path, default=ROOT / "artifacts/haworth-interchange/corpus"
    )
    parser.add_argument("--only")
    args = parser.parse_args()
    output = args.output.resolve()
    manifest_path = output / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    helper = ROOT / "artifacts/haworth-interchange/tools/capture-clipboard"
    for record in manifest["structures"]:
        if args.only and record["id"] != args.only:
            continue
        directory = output / record["id"]
        if args.mode == "reactions":
            reactions(directory, record)
        elif args.mode == "clipboard":
            clipboard(directory, record, helper)
        elif args.mode == "reshiki":
            reshiki(directory, record)
        else:
            reopen(output, record)
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
