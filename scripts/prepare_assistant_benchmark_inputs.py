"""Prepare original synthetic fixtures, independently checked with RDKit.
Images and native graphs are original MIT OR Apache-2.0 material; no model call.
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageEnhance
from rdkit import Chem, rdBase
from rdkit.Chem import rdMolDescriptors

project = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(project / "reference"))
from engine.worker import to_document  # noqa: E402 — reference path is established above

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--output", type=Path, default=project / "tests/fixtures/assistant-benchmark/v1"
)
parser.add_argument(
    "--renderer",
    type=Path,
    default=Path(
        os.environ.get(
            "RESHIKI_BENCHMARK_RENDERER", str(project / "target/debug/examples/assistant_smoke")
        )
    ),
)
args = parser.parse_args()
output = args.output
output.mkdir(parents=True, exist_ok=True)
renderer = args.renderer.resolve()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


originals = [
    ("ethanol", "CCO", "C2H6O", 3, 2, [], 1),
    ("gly-l-ala", "NCC(=O)N[C@@H](C)C(=O)O", "C5H10N2O3", 10, 9, ["S"], 1),
    ("cyclic-gly4", "N1CC(=O)NCC(=O)NCC(=O)NCC1=O", "C8H12N4O4", 16, 16, [], 1),
    ("boc-l-ala-sodium", "CC(C)(C)OC(=O)N[C@@H](C)C(=O)[O-].[Na+]", "C8H14NNaO4", 14, 12, ["S"], 2),
]
manifest = []
for name, smiles, formula, atoms, bonds, cip, fragments in originals:
    mol = Chem.MolFromSmiles(smiles)
    Chem.AssignStereochemistry(mol, cleanIt=True, force=True)
    assert rdMolDescriptors.CalcMolFormula(mol) == formula
    assert mol.GetNumAtoms() == atoms and mol.GetNumBonds() == bonds
    assert [code for _, code in Chem.FindMolChiralCenters(mol)] == cip
    assert len(Chem.GetMolFrags(mol)) == fragments
    if name == "gly-l-ala":
        # Original conventional zigzag with opposing carbonyls, avoiding label overlap.
        from rdkit.Geometry import Point3D

        coords = [
            (126, 0),
            (90, -21),
            (54, 0),
            (54, 42),
            (18, -21),
            (-18, 0),
            (-18, 42),
            (-54, -21),
            (-54, -63),
            (-90, 0),
        ]
        conf = Chem.Conformer(mol.GetNumAtoms())
        for idx, (x, y) in enumerate(coords):
            conf.SetAtomPosition(idx, Point3D(x / 28, -y / 28, 0))
        mol.AddConformer(conf)
    doc = to_document(mol)
    doc["version"] = 19
    if name == "boc-l-ala-sodium":
        doc["abbreviations"] = [
            {"label": "Boc", "anchor": 6, "members": list(range(1, 8)), "reverse_label": ""}
        ]
        sodium = doc["atoms"][-1]
        sodium["position"]["x"] = max(a["position"]["x"] for a in doc["atoms"][:-1]) + 70
        sodium["position"]["y"] = min(a["position"]["y"] for a in doc["atoms"][:-1]) + 30
    reference = {
        "smiles": smiles,
        "formula": formula,
        "heavy_atoms": atoms,
        "bonds": bonds,
        "tetrahedral_cip": cip,
        "fragments": fragments,
        "policy": "Exact protonation, formal charge, tautomer and specified stereochemistry; no tautomer/protonation equivalence.",
        "abbreviations": [{"label": "Boc", "smiles": "*C(=O)OC(C)(C)C"}]
        if name == "boc-l-ala-sodium"
        else [],
    }
    write_json(output / f"{name}.rsk", doc)
    write_json(output / f"{name}.reference.json", reference)
    renderdir = output / f"{name}-render"
    subprocess.run(
        [str(renderer), "--render", str(output / f"{name}.rsk"), "--output", str(renderdir)],
        check=True,
        capture_output=True,
    )
    data = (renderdir / "image-0.png").read_bytes()
    (output / f"{name}.png").write_bytes(data)
    im = Image.open(output / f"{name}.png")
    manifest.append(
        {
            "id": name,
            "image": f"{name}.png",
            "reference": f"{name}.reference.json",
            "width": im.width,
            "height": im.height,
            "image_sha256": hashlib.sha256(data).hexdigest(),
            "category": {
                "ethanol": "control",
                "gly-l-ala": "linear peptide stereo",
                "cyclic-gly4": "cyclic peptide macrocycle",
                "boc-l-ala-sodium": "abbreviation and disconnected salt",
            }[name],
            "source": "Original synthetic structure; independently authored reference SMILES, checked by RDKit formula/count/CIP/fragments and rendered as a ReShiki native drawing.",
            "license": "MIT OR Apache-2.0",
            "expected_response": "editable_draft",
        }
    )
variants = [
    (
        "gly-l-ala-low-resolution",
        "gly-l-ala",
        "half resolution",
        lambda im: im.resize(
            (max(1, im.width // 2), max(1, im.height // 2)), Image.Resampling.LANCZOS
        ),
        "editable_draft",
    ),
    (
        "cyclic-gly4-low-contrast",
        "cyclic-gly4",
        "40 percent contrast",
        lambda im: ImageEnhance.Contrast(im).enhance(0.4),
        "editable_draft",
    ),
    (
        "gly-l-ala-cropped",
        "gly-l-ala",
        "right 55 percent only; partial bonds/peptide missing",
        lambda im: im.crop((int(im.width * 0.45), 0, im.width, im.height)),
        "clarification_or_flagged_partial",
    ),
    (
        "ethanol-unreadable",
        "ethanol",
        "11 by 5 pixel image; atom labels no longer reliable",
        lambda im: im.resize((11, 5), Image.Resampling.LANCZOS),
        "clarification_or_flagged_partial",
    ),
]
for name, parent, transform, apply, response in variants:
    im = apply(Image.open(output / f"{parent}.png").convert("RGB"))
    path = output / f"{name}.png"
    im.save(path)
    manifest.append(
        {
            "id": name,
            "image": path.name,
            "reference": f"{parent}.reference.json" if response == "editable_draft" else None,
            "parent": parent,
            "transform": transform,
            "width": im.width,
            "height": im.height,
            "image_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "category": "resolution/contrast variation"
            if response == "editable_draft"
            else "ambiguous or unsupported readable graph",
            "source": "Deterministic variation of an original synthetic ReShiki image. No unique complete graph is scored for cropped/unreadable cases.",
            "license": "MIT OR Apache-2.0",
            "expected_response": response,
        }
    )
write_json(
    output / "manifest.json",
    {
        "dataset_version": 1,
        "reference_checker": f"RDKit {rdBase.rdkitVersion}",
        "source_renderer_sha256": hashlib.sha256(renderer.read_bytes()).hexdigest(),
        "cases": manifest,
    },
)
print(json.dumps({"cases": len(manifest), "output": str(output)}, indent=2))
