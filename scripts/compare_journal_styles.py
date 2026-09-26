#!/usr/bin/env python3
"""Capture real ChemDraw renders of the journal_styles example and build a report.

Requires ChemDraw for macOS. Only uniquely named scratch documents are changed.
No cleanup: molecular geometry and label positions are shared across renderers.
"""

import argparse
import html
import json
import plistlib
import re
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def quote(value):
    return '"' + str(value).replace("\\", "\\\\").replace('"', '\\"') + '"'


def capture(directory, record):
    folder = directory / record["id"]
    seed = folder / f"ReShiki-journal-{record['id']}.cdxml"
    seed.write_bytes((folder / "sample.cdxml").read_bytes())
    outputs = [
        ("svg", "Scalable Vector Graphics (SVG)"),
        ("pdf", "PDF"),
        ("cdxml", "ChemDraw XML"),
        ("cdx", "ChemDraw"),
    ]
    # Keep an object reference across Save As, which can change a document's name.
    saves = "\n".join(
        f"save ownedDocument in POSIX file {quote(folder / ('chemdraw.' + ext))} as {quote(fmt)}"
        for ext, fmt in outputs
    )
    script = f"""tell application "ChemDraw"
        open POSIX file {quote(seed)}
        set ownedDocument to document 1
        if name of ownedDocument does not contain "ReShiki-journal-" then error "Unexpected document; capture stopped"
        try
            {saves}
            close ownedDocument saving no
        on error messageText number messageNumber
            close ownedDocument saving no
            error messageText number messageNumber
        end try
    end tell"""
    subprocess.run(
        ["osascript", "-e", script], check=True, capture_output=True, text=True, timeout=45
    )
    for ext, _ in outputs:
        if not (folder / ("chemdraw." + ext)).stat().st_size:
            raise RuntimeError("Empty ChemDraw export: " + record["id"])
    # ChemDraw 26 writes dimensions to two decimal places in CDXML.
    # Verify within that serialization precision; preserve CDX for exact inspection.
    root = ET.parse(folder / "chemdraw.cdxml").getroot()
    keys = {
        "BondLength": "bond_length_pt",
        "LineWidth": "line_width_pt",
        "BoldWidth": "bold_width_pt",
        "MarginWidth": "margin_width_pt",
        "HashSpacing": "hash_spacing_pt",
        "LabelSize": "font_size_pt",
    }
    observed: dict[str, float | str] = {dest: float(root.attrib[src]) for src, dest in keys.items()}
    observed["bond_spacing_ratio"] = float(root.attrib["BondSpacing"]) / 100
    fonts = {f.attrib["id"]: f.attrib["name"] for f in root.findall("./fonttable/font")}
    observed["font_family"] = fonts[root.attrib["LabelFont"]]
    for key, value in observed.items():
        expected = record["style"][key]
        if (
            value != expected
            if isinstance(value, str)
            else abs(value - expected) > (0.00001 if key == "bond_spacing_ratio" else 0.0051)
        ):
            raise RuntimeError(f"ChemDraw changed {key} for {record['id']}: {expected} → {value}")
    return observed


def verify_binary(reader, directory, record):
    output = subprocess.check_output(
        [str(reader), "--inspect-style", str(directory / record["id"] / "chemdraw.cdx")],
        text=True,
        timeout=10,
    )
    observed = json.loads(output)
    for key in [
        "font_family",
        "font_size_pt",
        "bond_length_pt",
        "line_width_pt",
        "bold_width_pt",
        "margin_width_pt",
        "hash_spacing_pt",
        "bond_spacing_ratio",
    ]:
        value, expected = observed[key], record["style"][key]
        tolerance = 0.000001 if key == "bond_spacing_ratio" else 0.00002
        different = (
            value != expected if isinstance(value, str) else abs(value - expected) > tolerance
        )
        if different:
            raise RuntimeError(
                f"Binary ChemDraw setting changed: {record['id']} {key}: {expected} → {value}"
            )
    return observed


def svg_size(path, chemdraw=False):
    attrs = ET.parse(path).getroot().attrib

    def points(text):
        match = re.match(r"[\d.+eE-]+", text)
        if match is None:
            raise ValueError(f"Invalid SVG dimension: {text}")
        n = float(match.group())
        # ChemDraw exports SVG coordinates in points but labels its viewport px.
        # Its CDXML/PDF geometry confirms 1 viewBox unit = 1 publication point.
        if text.endswith("px") and not chemdraw:
            n *= 72 / 96
        return n

    return points(attrs["width"]), points(attrs["height"])


def report(directory, records, sources, version):
    escape = html.escape
    rows = []
    for record in records:
        slug = record["id"]
        style = record["style"]
        source = sources.get(slug)
        description = source["description"] if source else "Custom drawing style."
        cards = []
        for renderer, label in [("chemdraw", "ChemDraw"), ("reshiki", "ReShiki")]:
            width, height = svg_size(directory / slug / (renderer + ".svg"), renderer == "chemdraw")
            cards.append(
                f'''<div class="drawing"><div class="app">{label}</div><div class="viewport"><img alt="{label} rendering of aspirin, caffeine and alanine" src="{slug}/{renderer}.svg" data-width="{width}" data-height="{height}"></div><a href="{slug}/{renderer}.pdf">PDF</a> · <a href="{slug}/{renderer}.svg">SVG</a></div>'''
            )
        settings = " · ".join(
            [
                f"{style['font_family']} {style['font_size_pt']:g} pt",
                f"bond {style['bond_length_pt']:.4g} pt",
                f"line {style['line_width_pt']:.4g} pt",
                f"bold {style['bold_width_pt']:.4g} pt",
                f"margin {style['margin_width_pt']:.4g} pt",
                f"hash {style['hash_spacing_pt']:.4g} pt",
                f"spacing {style['bond_spacing_ratio'] * 100:g}%",
            ]
        )
        source_link = (
            f' · <a href="{escape(source["url"])}">Source</a>'
            if source and source.get("url")
            else ""
        )
        original_link = (
            f' · <a href="../sources/{escape(source["source_file"])}">Original stationery</a>'
            if source and source.get("kind") == "publisher template"
            else ""
        )
        rows.append(
            f'''<section data-preset="{slug}" data-bond="{style["bond_length_pt"]}"><h2>{escape(style["name"])}</h2><p>{escape(description)}{source_link}</p><p class="settings">{escape(settings)}</p><div class="pair">{"".join(cards)}</div><p class="files"><a href="{slug}/style.reshiki-style">Loadable preset</a> · <a href="{slug}/sample.rsk">ReShiki drawing</a> · <a href="{slug}/chemdraw.cdxml">ChemDraw drawing</a>{original_link}</p></section>'''
        )
    options = "".join(
        f'<option value="{r["id"]}">{escape(r["style"]["name"])}</option>' for r in records
    )
    page = """<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Journal drawing styles · ReShiki × ChemDraw</title><style>
:root{font-family:system-ui,-apple-system,sans-serif;color:#172b35;background:#eef2f4}*{box-sizing:border-box}body{margin:0}main{max-width:1440px;margin:auto;padding:36px}header{max-width:920px}h1{font-size:36px;letter-spacing:-1.1px;margin:8px 0 16px}h2{font-size:21px;margin:0}p{line-height:1.6}a{color:#075f78;text-underline-offset:3px}.kicker,.app{font-size:12px;letter-spacing:1px;text-transform:uppercase;color:#426172;font-weight:650}.controls{position:sticky;top:0;z-index:2;display:flex;flex-wrap:wrap;gap:18px;align-items:center;background:#eef2f4ed;backdrop-filter:blur(12px);padding:18px 0;margin:20px 0}select{max-width:330px;padding:8px;border:1px solid #9db0b9;border-radius:6px;background:white;font:inherit}input[type=range]{width:140px;vertical-align:middle}section{background:white;padding:24px;margin-bottom:22px;border:1px solid #d7e0e5;border-radius:12px;box-shadow:0 3px 12px #152c3506}section>p{margin:8px 0}.settings{font:13px ui-monospace,monospace;color:#4f6470}.pair{display:grid;grid-template-columns:1fr 1fr;gap:16px;margin:20px 0}.drawing{min-width:0;background:white;border:1px solid #e0e6e9;padding:16px;border-radius:6px}.viewport{overflow:auto;min-height:160px;display:flex;align-items:center;padding:22px 0}.viewport img{display:block;max-width:none;flex-shrink:0}.drawing>a,.files{font-size:13px}.note{border-left:3px solid #5b98a6;padding-left:16px;color:#425b69}footer{font-size:13px;color:#526974;margin-top:30px}[hidden]{display:none!important}@media(max-width:800px){main{padding:20px}.pair{grid-template-columns:1fr}h1{font-size:29px}}
</style><main><header><div class="kicker">Journal styles · 26 September 2026</div><h1>Same structures. Different drawing styles.</h1><p>Compare actual ChemDraw exports with ReShiki renders of aspirin, caffeine and alanine. Each preset follows publisher guidance or the publisher’s downloadable template. The same five journal presets are available in ReShiki’s Drawing style panel.</p><p class="note">Both apps receive the same coordinates and label positions. Default view uses a common physical scale; no image is fitted independently. “Equal bond length” isolates differences in proportions. Matching dimensions does not guarantee identical glyphs, label clearance or bond joins.</p></header><div class="controls"><label>Preset <select id="preset"><option value="all">All presets</option>OPTIONS</select></label><label>Zoom <input id="zoom" type="range" min="0.5" max="4" step="0.25" value="1"> <output id="zoom-value">1×</output></label><label><input id="normalize" type="checkbox"> Equal bond length</label></div>ROWS<footer>Rendered with ChemDraw VERSION and ReShiki 0.8.0 development branch. Numeric settings were checked against native ChemDraw CDX (within 0.00002 pt) as well as its rounded CDXML. ChemDraw SVG viewports are displayed in points to match their native CDXML/PDF geometry; original exports are retained. Font availability and renderer behavior can differ by platform. Template page layouts, colors and independent caption styles are outside this comparison. <a href="../sources/">Downloaded source files</a> · <a href="capture.json">Capture evidence</a></footer></main><script>
const select=document.querySelector('#preset'),zoom=document.querySelector('#zoom'),normalize=document.querySelector('#normalize');function update(){document.querySelector('#zoom-value').value=zoom.value+'×';for(const section of document.querySelectorAll('section')){section.hidden=select.value!=='all'&&select.value!==section.dataset.preset;const factor=Number(zoom.value)*96/72*(normalize.checked?14.4/Number(section.dataset.bond):1);for(const img of section.querySelectorAll('img')){img.style.width=img.dataset.width*factor+'px';img.style.height=img.dataset.height*factor+'px';}}}for(const control of [select,zoom,normalize])control.addEventListener('input',update);update();</script></html>"""
    page = (
        page.replace("OPTIONS", options)
        .replace("ROWS", "".join(rows))
        .replace("VERSION", escape(version))
    )
    (directory / "index.html").write_text(page)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--report-only", action="store_true")
    parser.add_argument(
        "--style-reader",
        type=Path,
        default=ROOT / "target/debug/examples/journal_styles",
        help="Built journal_styles example used to verify native CDX settings",
    )
    parser.add_argument("--only", help="Capture one preset, for inspection")
    args = parser.parse_args()
    directory = args.directory.resolve()
    records = json.loads((directory / "manifest.json").read_text())
    sources = {s["id"]: s for s in json.loads((ROOT / "presets/drawing/sources.json").read_text())}
    records = [record for record in records if record["id"] in sources]
    for record in records:
        record["style"]["name"] = sources[record["id"]]["name"]
    with open("/Applications/ChemDraw.app/Contents/Info.plist", "rb") as f:
        info = plistlib.load(f)
    version = info.get("CFBundleShortVersionString", "")
    evidence_path = directory / "capture.json"
    evidence = (
        json.loads(evidence_path.read_text())
        if evidence_path.exists()
        else {"version": version, "records": {}}
    )
    if not args.report_only:
        if not args.style_reader.is_file():
            parser.error("Build the journal_styles example first, or provide --style-reader")
        for record in records:
            if args.only and args.only != record["id"]:
                continue
            observed = capture(directory, record)
            binary = verify_binary(args.style_reader.resolve(), directory, record)
            evidence["records"][record["id"]] = {
                "cdxml": observed,
                "cdx": binary,
                "dimension_tolerance_pt": 0.00002,
                "spacing_tolerance_ratio": 0.000001,
            }
            evidence_path.write_text(json.dumps(evidence, indent=2) + "\n")
            print("Captured and verified:", record["id"], flush=True)
    if not args.only:
        report(directory, records, sources, version)
        print(directory / "index.html")


if __name__ == "__main__":
    main()
