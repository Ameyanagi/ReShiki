#!/usr/bin/env python3
"""Drive an installed ChemDraw on macOS to capture the Haworth export matrix.

Run examples/haworth_qa first. Outputs are deliberately outside version control:
    uv run --locked python scripts/chemdraw_haworth_corpus.py

Seed drawings contain geometry and visible bonds, never cached CIP assignments.
ChemDraw therefore supplies the stereochemical interpretation in the references.
The manifest distinguishes successful saves from independent identity checks.
"""

import argparse
import json
import subprocess
import time
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
# Save As / Export menus in ChemDraw 26.0.0.6599 for macOS, observed 2026-09-24.
FORMATS = [
    ("cdxml", "ChemDraw XML", "cdxml"),
    ("cdx", "ChemDraw", "cdx"),
    ("stationery", "ChemDraw Stationery", "cds"),
    ("chemdraw3", "ChemDraw 3.x", "chm"),
    ("pdf", "PDF", "pdf"),
    ("gif", "GIF", "gif"),
    ("bmp", "BMP", "bmp"),
    ("jpeg", "JPEG", "jpeg"),
    ("png", "PNG", "png"),
    ("tiff", "TIFF", "tiff"),
    ("eps", "Encapsulated PostScript", "eps"),
    ("tgf", "Transportable Graphics (TGF)", "tgf"),
    ("skc", "ISIS/Sketch (SKC)", "skc"),
    ("rxn", "Reaction Molfile (RXN)", "rxn"),
    ("rxn-v2000", "Reaction Molfile V2000 (RXN)", "rxn"),
    ("cml", "Chemical Markup Language (CML)", "cml"),
    ("ct", "Connection Table", "ct"),
    ("mol", "MDL Molfile", "mol"),
    ("mol-v2000", "MDL Molfile V2000", "mol"),
    ("sdf", "MDL SDfile", "sdf"),
    ("sdf-v2000", "MDL SDfile V2000", "sdf"),
    ("rdf", "MDL RDfile", "rdf"),
    ("rdf-v2000", "MDL RDfile V2000", "rdf"),
    ("msi", "MSI ChemNote", "msm"),
    ("smd", "SMD 4.2 File", "smd"),
    ("svg", "Scalable Vector Graphics (SVG)", "svg"),
]


def quote(value):
    return '"' + str(value).replace("\\", "\\\\").replace('"', '\\"') + '"'


def apple(script, timeout=20, alert_document=None, alert_contains=None, notices=None):
    process = subprocess.Popen(
        ["osascript", "-e", script], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
    )
    deadline = time.monotonic() + timeout
    alerts = []
    while True:
        try:
            stdout, stderr = process.communicate(timeout=0.3)
            break
        except subprocess.TimeoutExpired:
            if alert_document:
                # A failed save raises a native modal alert before the Apple
                # event returns. Read and acknowledge only this study's window.
                alert_script = f"""tell application "System Events" to tell process "ChemDraw"
                    if exists sheet 1 of window {quote(alert_document)} then
                        set s to sheet 1 of window {quote(alert_document)}
                        if exists button "OK" of s then
                            set message to value of every static text of s
                            click button "OK" of s
                            return message
                        end if
                    end if
                end tell"""
                observed = subprocess.run(
                    ["osascript", "-e", alert_script], capture_output=True, text=True, timeout=5
                )
                if observed.stdout.strip():
                    alerts.append(observed.stdout.strip())
            elif alert_contains:
                alert_script = f"""tell application "System Events" to tell process "ChemDraw"
                    repeat with w in windows
                        if subrole of w is "AXDialog" then
                            set message to value of every static text of w as text
                            if message contains {quote(alert_contains)} then
                                if exists button "Open" of w then
                                    click button "Open" of w
                                    return "OPEN: " & message
                                else if exists button "OK" of w then
                                    click button "OK" of w
                                    return "ERROR: " & message
                                end if
                            end if
                        end if
                        repeat with s in sheets of w
                            if exists button "OK" of s then
                                set message to value of every static text of s as text
                                if message contains {quote(alert_contains)} then
                                    click button "OK" of s
                                    return message
                                end if
                            end if
                        end repeat
                    end repeat
                end tell"""
                observed = subprocess.run(
                    ["osascript", "-e", alert_script], capture_output=True, text=True, timeout=5
                )
                if observed.stdout.startswith("OPEN: "):
                    if notices is not None:
                        notices.append(observed.stdout.strip()[6:])
                elif observed.stdout.strip():
                    alerts.append(observed.stdout.strip())
            if time.monotonic() >= deadline:
                process.kill()
                process.communicate()
                raise TimeoutError(f"ChemDraw did not answer within {timeout}s: {alerts}")
    if alerts or process.returncode:
        raise RuntimeError("; ".join(alerts + [stderr.strip()]).strip("; "))
    return stdout.strip().rstrip("\0")


def input_drawings(source):
    sugars = json.loads((ROOT / "assets/haworth-sugars.json").read_text())
    for sugar in sugars:
        slug = sugar["name"].replace("α", "alpha").replace("β", "beta")
        yield (
            slug,
            sugar["name"],
            json.loads((source / (sugar["name"] + ".rsk")).read_text()),
            sugar,
        )
    for size, name in [(5, "Furanose scaffold"), (6, "Pyranose scaffold")]:
        doc = json.loads((source / (name + ".rsk")).read_text())
        yield name.lower().replace(" ", "-"), name, doc, None
        carbon = json.loads(json.dumps(doc))
        for atom in carbon["atoms"]:
            atom["element"] = "C"
            atom["label_h"] = 0
        yield f"carbon-{size}-ring", f"{size}-membered carbon outline", carbon, None


def seed_xml(doc):
    root = ET.Element(
        "CDXML",
        BondLength="14.4",
        LineWidth="0.6",
        BoldWidth="2",
        LabelFont="3",
        LabelSize="10",
        LabelFace="0",
        CaptionFont="3",
        CaptionSize="10",
        MarginWidth="1.5",
        Magnification="200",
    )
    fonts = ET.SubElement(root, "fonttable")
    ET.SubElement(fonts, "font", id="3", charset="utf-8", name="Arial")
    page = ET.SubElement(root, "page", id="9000", BoundingBox="0 0 540 720")
    fragment = ET.SubElement(page, "fragment", id="9001")
    # C/O groups are expanded: the source graph is unchanged, with no text-only
    # substituent that could conceal a missing carbon or oxygen.
    positions = {a["id"]: dict(a["position"]) for a in doc["atoms"]}
    for group in doc.get("abbreviations", []):
        if group["label"] != "CH2OH" or len(group["members"]) != 2:
            continue
        anchor = group["anchor"]
        outside = [
            b["b"] if b["a"] == anchor else b["a"]
            for b in doc["bonds"]
            if anchor in (b["a"], b["b"])
            and (b["b"] if b["a"] == anchor else b["a"]) not in group["members"]
        ]
        if len(outside) != 1:
            raise ValueError("Ambiguous hydroxymethyl attachment")
        origin, parent = positions[anchor], positions[outside[0]]
        delta = origin["y"] - parent["y"]
        terminal = next(i for i in group["members"] if i != anchor)
        # Expanded terminal OH points away from the ring; its bond rotation
        # changes neither the ring substituent direction nor stereochemistry.
        positions[terminal] = {
            "x": origin["x"] + (1 if origin["x"] > 0 else -1) * abs(delta) * 0.8,
            "y": origin["y"] + delta * 0.6,
        }
    for atom in doc["atoms"]:
        pos = positions[atom["id"]]
        x, y = 150 + pos["x"] * 14.4 / 42, 150 + pos["y"] * 14.4 / 42
        node = ET.SubElement(fragment, "n", id=str(atom["id"]), p=f"{x:.8f} {y:.8f}")
        if atom["element"] == "O":
            placement = atom.get("display", {}).get("hydrogen_position", "auto")
            node.set("LabelDisplay", placement.capitalize())
            node.set("Element", "8")
            node.set("NumHydrogens", str(atom.get("label_h", 0)))
            text = ET.SubElement(node, "t", p=f"{x:.8f} {y + 3.5:.8f}", Justification="Left")
            ET.SubElement(text, "s", font="3", size="10", face="96").text = (
                "OH" if atom.get("label_h", 0) else "O"
            )
    for i, bond in enumerate(doc["bonds"]):
        ET.SubElement(
            fragment,
            "b",
            id=str(100 + i),
            B=str(bond["a"]),
            E=str(bond["b"]),
            Order="1",
            Display={"wedge": "WedgeBegin", "bold": "Bold", "plain": "Solid"}[bond["display"]],
        )
    return ET.tostring(root, encoding="unicode", xml_declaration=True)


def capture(output, source, only):
    output.mkdir(parents=True, exist_ok=True)
    manifest_path = output / "manifest.json"
    manifest: dict[str, Any] = {
        "application": "ChemDraw 26.0.0.6599 for macOS",
        "date": time.strftime("%Y-%m-%d"),
        "method": "ChemDraw AppleScript save; file types inventoried through its native menus",
        "seed": "Expanded ReShiki graphs and Haworth geometry; no AS/Geometry/BondOrdering caches",
        "formats": [{"id": i, "menu": n, "extension": e} for i, n, e in FORMATS],
        "structures": [],
    }
    if only and manifest_path.exists():
        manifest = json.loads(manifest_path.read_text())
    for slug, name, doc, expected in input_drawings(source):
        if only and slug != only:
            continue
        directory = output / slug
        directory.mkdir(exist_ok=True)
        seed = directory / "input.cdxml"
        seed.write_text(seed_xml(doc))
        record = {"id": slug, "name": name, "expected": expected, "exports": []}
        previous = next((i for i, r in enumerate(manifest["structures"]) if r["id"] == slug), None)
        if previous is None:
            manifest["structures"].append(record)
        else:
            manifest["structures"][previous] = record
        apple(f'tell application "ChemDraw" to open POSIX file {quote(seed)}')
        doc_name = apple('tell application "ChemDraw" to get name of document 1')
        d = "document " + quote(doc_name)
        try:
            apple(
                'tell application "ChemDraw" to activate\n'
                'tell application "System Events" to tell process "ChemDraw" '
                'to keystroke "a" using command down'
            )
            record["smiles"] = apple(
                f'tell application "ChemDraw" to get SMILES of selection of {d}'
            )
            record["formula"] = apple(
                f'tell application "ChemDraw" to get Molecular Formula of selection of {d}'
            )
            record["atoms"] = int(apple(f'tell application "ChemDraw" to count atoms of {d}'))
            record["bonds"] = int(apple(f'tell application "ChemDraw" to count bonds of {d}'))
            for format_id, format_name, extension in FORMATS:
                path = directory / f"{format_id}.{extension}"
                entry = {"format": format_id, "path": str(path.relative_to(output))}
                record["exports"].append(entry)
                try:
                    apple(
                        f'tell application "ChemDraw" to save {d} '
                        f"in POSIX file {quote(path)} as {quote(format_name)}",
                        alert_document=doc_name,
                    )
                    entry["bytes"] = path.stat().st_size
                    entry["status"] = "saved" if entry["bytes"] else "empty"
                except (RuntimeError, OSError, subprocess.TimeoutExpired) as error:
                    entry["status"] = "failed"
                    entry["error"] = str(error)
                manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
                print(
                    slug,
                    format_id,
                    entry["status"],
                    entry.get("bytes", entry.get("error")),
                    flush=True,
                )
        finally:
            apple(f'tell application "ChemDraw" to close {d} saving no')
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=ROOT / "artifacts/haworth-qa")
    parser.add_argument(
        "--output", type=Path, default=ROOT / "artifacts/haworth-interchange/corpus"
    )
    parser.add_argument("--only", help="Capture one structure by its ASCII id")
    args = parser.parse_args()
    capture(args.output.resolve(), args.source.resolve(), args.only)
