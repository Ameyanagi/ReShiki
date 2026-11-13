"""Original SMILES import responses, including layout and full CIP labels."""

import json
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.worker import from_document, handle

if TYPE_CHECKING or __package__:
    from .smiles_read_reference import cases
    from .valence_reference import ORDERS
else:
    from smiles_read_reference import cases
    from valence_reference import ORDERS

# ReShiki keeps RDKit's legacy ring symmetrization; child processes inherit it.
Chem.SetUseLegacyRingFinding(True)


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    for index, (name, text) in enumerate(cases()):
        # Complete parser/state comparisons have their own unsampled suite.
        # Here keep all templates, CX interactions, import names and optional
        # source fixtures, plus a deterministic sample of the larger corpus.
        if not name.startswith(("template/", "interactions/", "names/", "source", "hydrogen/")):
            if index % 31:
                continue
        try:
            expected = handle(dict(protocol=1, operation="import", format="smiles", text=text))
            failure = None
        except (ValueError, RuntimeError, KeyError, OverflowError) as error:
            expected, failure = None, str(error)
        print(
            json.dumps(
                dict(
                    name=name,
                    text=text,
                    expected=expected,
                    failure=failure,
                    sssr=sssr(expected),
                    unspecified_ez=unspecified_ez(text) if expected else None,
                    coordinate_wedges=coordinate_wedges(text) if expected else None,
                )
            )
        )


def unspecified_ez(text):
    """#909/#923: double bonds whose configuration the input leaves open (and
    every cumulated double bond), with RDKit's identifiers when the new layout
    does not define them. The original worker reads E/Z from that layout."""
    mol = Chem.MolFromSmiles(text)
    # RDKit drops slash E/Z when CX wedge flags are present, while the worker
    # keeps it from the drawn layout; leave those inputs unadapted.
    if mol is None or wedge_flags(text):
        return None
    cumulated = {
        a.GetIdx()
        for a in mol.GetAtoms()
        if sum(b.GetBondType() == Chem.BondType.DOUBLE for b in a.GetBonds()) == 2
    }
    bonds = [
        b.GetIdx()
        for b in mol.GetBonds()
        if b.GetBondType() == Chem.BondType.DOUBLE
        and (
            b.GetStereo() == Chem.BondStereo.STEREONONE
            or {b.GetBeginAtomIdx(), b.GetEndAtomIdx()} & cumulated
        )
    ]
    for i in bonds:
        mol.GetBondWithIdx(i).SetStereo(Chem.BondStereo.STEREONONE)
    mol.RemoveAllConformers()
    return dict(bonds=bonds, inchi=Chem.MolToInchi(mol), inchikey=Chem.MolToInchiKey(mol))


# ReShiki's bond order codes (valence_reference.py) by RDKit bond type.
CODES = {kind: code for code, kind in ORDERS.items()}


def coordinate_wedges(text):
    """#1045: import drops input wedge flags before the fresh layout and wedges
    the molecule again from its stereo state. The original worker keeps the
    flags on its new layout, where they can describe other stereo. RDKit's
    reading of the input itself gives the identifiers."""
    _, *cx = text.strip().split(maxsplit=1)
    cx = cx[0] if cx else ""
    if "(" not in cx or not any(flag in cx for flag in ("wU:", "wD:")):
        return None
    mol = Chem.MolFromSmiles(text)
    if mol is None:
        return None
    # RDKit's Kekulize weighs wedged bonds; with the input wedges cleared, as
    # #1045 import does, it gives the Kekule form ReShiki draws.
    kekule = Chem.Mol(mol)
    for bond in kekule.GetBonds():
        bond.SetBondDir(Chem.BondDir.NONE)
    try:
        Chem.Kekulize(kekule, clearAromaticFlags=True)
    except (ValueError, RuntimeError):
        kekule = Chem.Mol()
    return dict(
        inchi=Chem.MolToInchi(mol),
        inchikey=Chem.MolToInchiKey(mol),
        smiles=Chem.MolToSmiles(mol),
        kekule=[
            [b.GetBeginAtomIdx(), b.GetEndAtomIdx(), CODES[b.GetBondType()]]
            for b in kekule.GetBonds()
            if b.GetBondType() in CODES
        ],
    )


def wedge_flags(text):
    """Slash E/Z next to CX wedge flags, which RDKit drops."""
    smiles, *cx = text.strip().split(maxsplit=1)
    cx = cx[0] if cx else ""
    return any(flag in cx for flag in ("wU:", "wD:", "w:")) and any(c in smiles for c in "/\\")


def sssr(expected):
    """RDKit 2026.09's SSSR basis for the drawing, in document atom order
    (rdkit#9105); engine_migration compares it with ReShiki's basis."""
    if not expected or not expected.get("document"):
        return None
    try:
        mol = from_document(expected["document"])
    except (ValueError, RuntimeError):  # The worker cannot read every drawing it returns.
        return None
    # Layout rings include dative bonds, as RingInfo does.
    return sorted(sorted(ring) for ring in Chem.GetSSSR(mol, includeDativeBonds=True))


if __name__ == "__main__":
    main()
