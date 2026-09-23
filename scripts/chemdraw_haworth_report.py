#!/usr/bin/env python3
"""Make the review gallery and integrity/identity report from actual captures."""

import hashlib
import html
import json
import xml.etree.ElementTree as ET
from urllib.parse import quote

from chemdraw_haworth_corpus import ROOT
from rdkit import Chem

DIRECTORY = ROOT / "artifacts/haworth-interchange/corpus"


def gallery(manifest):
    first = manifest["structures"][0]["id"]
    root = ET.parse(DIRECTORY / first / "cdxml.cdxml").getroot()
    for page in root.findall("page"):
        root.remove(page)
    root.set("BoundingBox", "0 0 800 730")
    root.set("Magnification", "125")
    page = ET.SubElement(
        root, "page", id="999999", BoundingBox="0 0 1046 770", HeightPages="1", WidthPages="2"
    )
    for i, record in enumerate(manifest["structures"]):
        fragment = ET.parse(DIRECTORY / record["id"] / "cdxml.cdxml").find("page/fragment")
        if fragment is None:
            raise ValueError("Missing ChemDraw fragment")
        offset = (i + 1) * 10000
        dx, dy = 105 + i % 4 * 195 - 150, 100 + i // 4 * 170 - 150
        for node in fragment.iter():
            for attr in ["id", "B", "E"]:
                if attr in node.attrib:
                    node.set(attr, str(int(node.attrib[attr]) + offset))
            if "BondOrdering" in node.attrib:
                node.set(
                    "BondOrdering",
                    " ".join(
                        str(int(v) + offset) if int(v) else "0"
                        for v in node.attrib["BondOrdering"].split()
                    ),
                )
            for attr in ["p", "BoundingBox"]:
                if attr in node.attrib:
                    values = [
                        float(v) + (dy if j % 2 else dx)
                        for j, v in enumerate(node.attrib[attr].split())
                    ]
                    node.set(attr, " ".join(f"{v:.8f}" for v in values))
        page.append(fragment)
        label = ET.SubElement(page, "t", id=str(offset + 9999), p=f"{dx + 82} {dy + 220}")
        ET.SubElement(label, "s", font="60", size="10").text = record["name"]
    (DIRECTORY / "all-haworth-input.cdxml").write_text(
        ET.tostring(root, encoding="unicode", xml_declaration=True)
    )


def key(smiles):
    # The RXN test containers have one reactant, no products, and no claimed
    # transformation. Compare their reactant identity, not a reaction string.
    molecule = Chem.MolFromSmiles(smiles.split(">")[0]) if smiles else None
    return Chem.MolToInchiKey(molecule) if molecule is not None else None


def main():
    manifest = json.loads((DIRECTORY / "manifest.json").read_text())
    if len(manifest["structures"]) != 14:
        raise ValueError("The complete report requires all fourteen structures")
    for record in manifest["structures"]:
        saved = [
            e
            for e in record["exports"] + record.get("reaction_exports", [])
            if e["status"] == "saved"
        ]
        if len(saved) != 26 or len(record.get("clipboard", [])) != 10:
            raise ValueError(f"Incomplete export matrix for {record['id']}")
    gallery(manifest)
    checks = []
    cards = []
    formats = {f["id"]: f["menu"] for f in manifest["formats"]}
    for record in manifest["structures"]:
        expected = record["expected"]
        expected_key = expected["inchikey"] if expected else key(record["smiles"])
        links = []
        for entry in record["exports"] + record.get("reaction_exports", []):
            check = {"structure": record["id"], **entry, "expected_inchikey": expected_key}
            if entry["status"] == "saved":
                path = DIRECTORY / entry["path"]
                check["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
                reopened = entry.get("reopen", {})
                observed = key(reopened.get("smiles", ""))
                if entry["format"] == "svg" and reopened.get("status") == "not-imported":
                    result = "SVG export only"
                    if (DIRECTORY / record["id"] / "svg-render-check.png").exists():
                        result += " · render verified"
                elif reopened.get("status") != "opened":
                    result = "Reopen not verified"
                elif reopened.get("atoms") == 0:
                    result = "Opened as artwork"
                elif observed == expected_key:
                    result = "Identity matches"
                else:
                    result = "IDENTITY MISMATCH"
                check.update(result=result, observed_inchikey=observed)
                title = formats[entry["format"]]
                if "context" in entry:
                    title += " · reactant container"
                links.append(
                    f'<li><a href="{quote(entry["path"])}">{html.escape(title)}</a>'
                    f"<small>{result}</small></li>"
                )
            checks.append(check)
        clipboard = []
        for entry in record.get("clipboard", []):
            if entry["status"] == "captured":
                folder = DIRECTORY / entry["path"]
                files = []
                for representation in entry["types"]:
                    p = folder / representation["file"]
                    checks.append(
                        {
                            "structure": record["id"],
                            "clipboard": entry["format"],
                            **representation,
                            "path": str(p.relative_to(DIRECTORY)),
                            "sha256": hashlib.sha256(p.read_bytes()).hexdigest(),
                        }
                    )
                    files.append(
                        f'<a href="{quote(str(p.relative_to(DIRECTORY)))}">'
                        f"{html.escape(representation['type'])}</a>"
                    )
                clipboard.append(
                    f"<li>{html.escape(entry['format'])}: " + " · ".join(files) + "</li>"
                )
            else:
                clipboard.append(f"<li>{html.escape(entry['format'])}: {entry['status']}</li>")
        reverse = []
        for entry in record.get("reshiki_exports", []):
            observed = key(entry["reopen"].get("smiles", ""))
            result = "Identity matches" if observed == expected_key else "IDENTITY MISMATCH"
            checks.append(
                {
                    "structure": record["id"],
                    "reshiki_export": entry["format"],
                    "result": result,
                    "observed_inchikey": observed,
                    "expected_inchikey": expected_key,
                }
            )
            reverse.append(
                f'<li><a href="{quote(entry["path"])}">ReShiki {entry["format"].upper()}</a>'
                f"<small>{result} in ChemDraw</small></li>"
            )
        cards.append(f'''<article data-name="{html.escape(record["name"].lower())}">
          <h2>{html.escape(record["name"])}</h2>
          <a href="{record["id"]}/png.png"><img src="{record["id"]}/png.png" alt="ChemDraw export of {html.escape(record["name"])}"></a>
          <p>{record["atoms"]} atoms · {record["bonds"]} bonds</p>
          <code>{expected_key}</code>
          <details><summary>26 ChemDraw file exports</summary><ul>{"".join(links)}</ul></details>
          <details><summary>Clipboard formats</summary><ul>{"".join(clipboard)}</ul></details>
          <details><summary>ReShiki → ChemDraw</summary><ul>{"".join(reverse)}</ul></details>
        </article>''')
    (DIRECTORY / "validation.json").write_text(
        json.dumps(checks, ensure_ascii=False, indent=2) + "\n"
    )
    failures = [c for c in checks if c.get("result") == "IDENTITY MISMATCH"]
    body = (
        """<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width">
    <title>Haworth interchange · ReShiki</title><style>
    *{box-sizing:border-box}body{margin:0;background:#f5f5f1;color:#24312c;font:16px/1.55 system-ui,sans-serif}
    header,main{max-width:1440px;margin:auto;padding:32px}header{padding-bottom:12px}
    h1{font-size:36px;line-height:1.15;margin:8px 0 16px}h2{font-size:18px;margin:0 0 8px}
    p{max-width:920px}a{color:#944427}input{width:100%;max-width:460px;padding:12px;border:1px solid #9ba9a2;border-radius:8px;font:inherit}
    main{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:20px}
    article{background:white;border:1px solid #d2dad5;border-radius:12px;padding:20px;align-self:start}
    img{display:block;background:white;width:100%;height:240px;object-fit:contain}code{font-size:11px;word-break:break-all}
    details{border-top:1px solid #e3e7e3;padding:12px 0 0;margin-top:14px}summary{cursor:pointer;font-weight:600}
    ul{padding-left:20px}li{margin:10px 0;overflow-wrap:anywhere}small{display:block;color:#496457;font-size:12px}
    .eyebrow{color:#944427;font-size:14px;font-weight:650}article[hidden]{display:none}
    </style><header><div class="eyebrow">ReShiki · ChemDraw 26.0.0.6599 · 24 September 2026</div>
    <h1>All 14 Haworth drawings.<br>Every available export format.</h1>
    <p>Ten named α/β D-sugars, two oxygen scaffolds and two carbon outlines. The previews below are actual
    ChemDraw PNG exports. Each card links to the files saved by ChemDraw and the results of reopening them.</p>
    <p>26 file formats × 14 drawings = 364 generated exports. RXN uses a single-reactant container with an arrow,
    without claiming a chemical transformation. Eight clipboard options work for each drawing; ChemDraw disables
    both 3MF options for these 2D structures. “Opened as artwork” does not imply editable chemistry.</p>
    <p><a href="manifest.json">Capture manifest</a> · <a href="validation.json">Identity checks and SHA-256 hashes</a></p>
    <label for="search">Find a structure</label><br><input id="search" type="search" placeholder="Glucose, ribose, scaffold…"></header>
    <main>"""
        + "".join(cards)
        + """</main><script>
    document.getElementById('search').addEventListener('input',event=>{
      const q=event.target.value.toLowerCase();document.querySelectorAll('article').forEach(card=>card.hidden=!card.dataset.name.includes(q));
    });</script></html>"""
    )
    (DIRECTORY / "index.html").write_text(body)
    print(
        f"{len(manifest['structures'])} structures; {len(checks)} records; {len(failures)} identity mismatches"
    )
    for failure in failures:
        print(failure)


if __name__ == "__main__":
    main()
