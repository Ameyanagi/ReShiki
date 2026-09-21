"""Independent RDKit ring oracle: examples, permutations and bundled NCI data."""

import itertools
import json
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .valence_reference import graph
else:
    from valence_reference import graph


def emit(name, mol, dative=False, hydrogen=False):
    # Use separate copies: GetSSSR/GetSymmSSSR overwrite RDKit's ring cache.
    source = graph(mol)
    try:
        basis = list(Chem.GetSSSR(Chem.Mol(mol), dative, hydrogen))
        sym_mol = Chem.Mol(mol)
        sym = list(Chem.GetSymmSSSR(sym_mol, dative, hydrogen))
        rings = sorted(sorted(r) for r in sym)
        bonds = sorted(sorted(r) for r in sym_mol.GetRingInfo().BondRings())
        expected = dict(basis_count=len(basis), atoms=rings, bonds=bonds)
    except (ValueError, RuntimeError):
        expected = None
    print(
        json.dumps(
            dict(name=name, graph=source, dative=dative, hydrogen=hydrogen, expected=expected)
        )
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.Mol())
    # Ring regressions adapted from RDKit molopstest.cpp (BSD-3-Clause).
    examples = [
        "C1CC1",
        "C1CCC1",
        "C1CCCCCC1",
        "C1C(CCC)CC(C(C)CCC(CC))CCC1",
        "CC1C(C2)CCC2C1",
        "C(C1C2C3C41)(C2C35)C45",
        "C12CC(CC2)CC1",
        "C123C4C5C6(C3)C7C1C8C2C4C5C6C78",
        "C1CC2C1CCC2",
        "C12=C3C=CC=C1C=CC2=CC=C3",
        "SC(C3C1CC(C3)CC(C2S)(O)C1)2S",
        "CC1=CC=C(C=C1)S(=O)(=O)O[CH]2[CH]3CO[CH](O3)[CH]4OC(C)(C)O[CH]24",
        "C1CC2C1C2",
        "C=C1C2CC1C2",
        "C1C4C5C3C(=O)C2C5C1C2C34",
        "C12=CON=C1C(C4)CC3CC2CC4C3",
        "C17C5C4C3C2C1C6C2C3C4C5C67",
        "C1CCCCC1.C12C3C4C1C5C2C3C45",
        "C1CCC2(CC1)CCCC2",
        "C" * 2000,
    ]
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    examples += [t["smiles"] for t in templates if t.get("smiles")]
    rng = random.Random(74133)
    for text in examples:
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(text)
        emit(text, mol)
        for i in range(4):
            permutation = list(range(mol.GetNumAtoms()))
            rng.shuffle(permutation)
            emit(f"{text} permuted {i}", Chem.RenumberAtoms(mol, permutation))
    sample = Path(RDConfig.RDDataDir) / "NCI/first_5K.smi"
    for line in sample.read_text().splitlines():
        text, name = line.split()
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(f"Invalid NCI syntax: {name}")
        emit(f"NCI {name}", mol)
        emit(
            f"NCI {name} reversed",
            Chem.RenumberAtoms(mol, list(reversed(range(mol.GetNumAtoms())))),
        )
    # Topological graphs isolate ring behavior from unrelated valence rejection.
    for case in range(600):
        n = rng.randrange(3, 23)
        mol = Chem.RWMol()
        for _ in range(n):
            mol.AddAtom(Chem.Atom(0))
        edges = list(itertools.combinations(range(n), 2))
        rng.shuffle(edges)
        for a, b in edges[: rng.randrange(n - 1, min(len(edges), n * 2) + 1)]:
            mol.AddBond(a, b, Chem.BondType.SINGLE)
        emit(f"topology {case}", mol)
    for text in ("C1CC1", "C1CCC2(CC1)CCCC2", "C12C3C4C1C5C2C3C45", "C1CC1.C1CC1"):
        for kind in (Chem.BondType.DATIVE, Chem.BondType.HYDROGEN):
            mol = Chem.MolFromSmiles(text, sanitize=False)
            mol.GetBondWithIdx(0).SetBondType(kind)
            for dative, hydrogen in itertools.product((False, True), repeat=2):
                emit(f"special {text} {kind} {dative}/{hydrogen}", mol, dative, hydrogen)


if __name__ == "__main__":
    main()
