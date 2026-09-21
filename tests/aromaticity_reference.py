"""Independent RDKit aromaticity and post-aromaticity hydrogen adjustment."""

import json
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .valence_reference import ORDERS, graph
else:
    from valence_reference import ORDERS, graph


def emit(name, mol, adjust=False):
    mol.UpdatePropertyCache(strict=False)
    Chem.GetSymmSSSR(mol)
    source = graph(mol)
    rings = list(mol.GetRingInfo().AtomRings())
    original = Chem.Mol(mol)
    try:
        Chem.SetAromaticity(mol)
        if adjust:
            mol = original
            Chem.SanitizeMol(
                mol,
                sanitizeOps=Chem.SanitizeFlags.SANITIZE_SETAROMATICITY
                | Chem.SanitizeFlags.SANITIZE_ADJUSTHS,
            )
        expected = dict(graph=graph(mol), aromatic_rings=mol.GetIntProp("numArom"))
    except (ValueError, RuntimeError):
        expected = None
    print(json.dumps(dict(name=name, graph=source, rings=rings, adjust=adjust, expected=expected)))


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.RWMol())
    # Vary donor types, charges, radicals, ring sizes, and exocyclic bonds.
    for size in range(3, 15):
        for number in (0, 5, 6, 7, 8, 15, 16, 34, 52):
            for charge in (-2, -1, 0, 1, 2):
                for radical in (0, 1):
                    for external in (None, 6, 7, 8, 16):
                        mol = Chem.RWMol()
                        for i in range(size):
                            atom = Chem.Atom(number if i == 0 else 6)
                            if i == 0:
                                atom.SetFormalCharge(charge)
                                atom.SetNumRadicalElectrons(radical)
                            mol.AddAtom(atom)
                        for i in range(size):
                            kind = Chem.BondType.DOUBLE if i % 2 else Chem.BondType.SINGLE
                            mol.AddBond(i, (i + 1) % size, kind)
                        if external is not None:
                            end = mol.AddAtom(Chem.Atom(external))
                            mol.AddBond(0, end, Chem.BondType.DOUBLE)
                        emit(f"ring {size}/{number}/{charge}/{radical}/{external}", mol)
    for text in ("C1=CC=CC=C1", "N1C=CC=C1", "S1C=CC=C1"):
        for order in (0, 3, 5, 6, 7):
            for reverse in (False, True):
                mol = Chem.RWMol(Chem.MolFromSmiles(text, sanitize=False))
                metal = mol.AddAtom(Chem.Atom(26))
                a, b = (metal, 0) if reverse else (0, metal)
                mol.AddBond(a, b, ORDERS[order])
                emit(f"external bond {text}/{order}/{reverse}", mol)
    texts = [
        "c1ccccc1",
        "c1cc[nH]c1",
        "c1ccc2occc2c1",
        "O=C1NC=CC2=C1C=CC=C2",
        "c1ccc2ccccc2c1",
        "c1cc2ccc3cccc4ccc(c1)c2c34",
        "O=C1C=CC(=O)C=C1",
        "C1=CC=CC=CC=CC=C1",
        "C1=CC=CC=C1C2=CC=CC=C2",
        "C1=CC=[N+]([O-])C=C1",
        "C1=C=NC=N1",
        "C1CC=CCOCC=CC1",
        "C1CC=CCSCC=CC1",
        "[CH+]1C=C1",
        "[CH+]1C=CC=CC=C1",
        "[CH-]1C=CC=C1",
        "[N]1C=CC=C1",
        "[C]1=CC=CC=C1",
        "C12C3C4C1C5C2C3C45",
        "*1=CC=CC=C1",
        "*1:*:*:*:*:*:1",
        "C1CCC2(CC1)CCCC2",
        "[se]1cccc1",
        "[te]1cccc1",
        # RDKit regressions: acepentalene's buried atom, a fused central N ring,
        # cyclic triples, carbonyl electron withdrawal, and carbon/hetero radicals.
        "C1=CC2=CC=C3C2=C1C=C3",
        "C1=CN2C3=CC=CN3C3=CC=CN3C2=C1",
        "C1#CC=C1",
        "C1#CC=CC=C1",
        "O=C1C(=O)C=C1",
        "C1=CC(=C)C(=C)C=C1",
        "[C+]1=CNC=N1",
        "c1cccc[n+]1",
        "[n]1ccccc1",
        "[H]n1cccc1",
    ]
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    texts += [t["smiles"] for t in templates if t.get("smiles")]
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    rng = random.Random(4524)
    for text in texts:
        raw = Chem.MolFromSmiles(text, sanitize=False)
        if raw is None:
            raise ValueError(f"Invalid corpus syntax: {text}")
        emit(text + " raw", Chem.Mol(raw))
        mol = Chem.Mol(raw)
        try:
            Chem.SanitizeMol(mol)
            Chem.Kekulize(mol, clearAromaticFlags=True)
        except (ValueError, RuntimeError):
            # Retain the raw case above, including rejected valences.
            continue
        emit(text + " kekule", Chem.Mol(mol))
        emit(text + " adjusted H", Chem.Mol(mol), adjust=True)
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(text + " permuted", Chem.RenumberAtoms(mol, order))
        emit(text + " graph H", Chem.AddHs(mol))


if __name__ == "__main__":
    main()
