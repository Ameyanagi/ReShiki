"""Independent RDKit atom/atropisomer cleanup, including stereo-group metadata."""

import json
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .kekulize_reference import directions
    from .ranking_reference import metadata
    from .valence_reference import atom_molecule, graph
else:
    from kekulize_reference import directions
    from ranking_reference import metadata
    from valence_reference import atom_molecule, graph


def state(mol):
    return dict(
        graph=graph(mol),
        metadata=metadata(mol),
        directions=directions(mol),
        hybridizations=[str(a.GetHybridization()) for a in mol.GetAtoms()],
        conjugated=[b.GetIsConjugated() for b in mol.GetBonds()],
    )


def emit(name, original, operation="chirality", basis=False):
    mol = Chem.Mol(original)
    mol.UpdatePropertyCache(strict=False)
    mol.ClearComputedProps(includeRings=True)
    mol.UpdatePropertyCache(strict=False)
    if basis:
        Chem.GetSSSR(mol)
    else:
        Chem.GetSymmSSSR(mol)
    rings = list(mol.GetRingInfo().AtomRings())
    before = state(mol)
    try:
        if operation in ("atrop", "both"):
            Chem.CleanupAtropisomers(mol)
        if operation in ("chirality", "both"):
            Chem.CleanupChirality(mol)
        expected = state(mol)
    except (ValueError, RuntimeError):
        expected = None
    print(
        json.dumps(
            dict(
                name=f"{name}/{operation}/{basis}",
                operation=operation,
                rings=rings,
                before=before,
                expected=expected,
            )
        )
    )


def group(mol, kind, atoms, bonds, identity):
    result = Chem.CreateStereoGroup(
        Chem.StereoGroupType.values[kind], mol, atomIds=atoms, bondIds=bonds, readId=identity
    )
    result.SetWriteId(identity + 500)
    return result


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.Mol(), "both")
    for number in (0, 6, 7, 15, 78):
        for tag in range(9):
            limit = {4: 2, 6: 3, 7: 20, 8: 30}.get(tag, 2)
            for degree in range(8):
                for hydrogens in (0, 1):
                    for hybrid in range(9):
                        for permutation in (None, 0, limit, limit + 1, 2**32 - 1):
                            mol = atom_molecule(number, 0, hydrogens, 0, True)
                            atom = mol.GetAtomWithIdx(0)
                            atom.SetChiralTag(Chem.ChiralType.values[tag])
                            atom.SetHybridization(Chem.HybridizationType.values[hybrid])
                            if permutation is not None:
                                atom.SetUnsignedProp("_chiralPermutation", permutation)
                            for _ in range(degree):
                                other = mol.AddAtom(Chem.Atom(6))
                                mol.AddBond(0, other, Chem.BondType.SINGLE)
                            mol.SetStereoGroups([group(mol, tag % 3, [0], [], 43)])
                            emit(
                                f"atom {number}/{tag}/{degree}/{hydrogens}/{hybrid}/{permutation}",
                                mol,
                            )
    # Mixed groups exercise partial retention, bond-only groups, and group IDs.
    rng = random.Random(84173)
    for sample in range(2000):
        mol = Chem.RWMol(Chem.MolFromSmiles("CC(F)(Cl)C=C(Br)C"))
        for atom in mol.GetAtoms():
            atom.SetChiralTag(Chem.ChiralType.values[rng.randrange(9)])
            atom.SetHybridization(Chem.HybridizationType.values[rng.randrange(9)])
            if rng.random() < 0.7:
                atom.SetUnsignedProp("_chiralPermutation", rng.choice((0, 1, 3, 20, 31)))
        for bond in mol.GetBonds():
            bond.SetStereo(Chem.BondStereo.values[rng.choice((0, 1, 6, 7))])
            bond.SetBondDir(Chem.BondDir.values[rng.randrange(7)])
        groups = []
        for kind in range(3):
            atoms = rng.sample(list(range(mol.GetNumAtoms())), rng.randrange(1, 5))
            bonds = rng.sample(list(range(mol.GetNumBonds())), rng.randrange(1, 4))
            # ROMol merges ABS groups; overlapping ABS membership is invalid.
            groups.append(
                group(mol, kind, [] if kind == 0 and sample % 5 == 0 else atoms, bonds, kind + 7)
            )
            if kind != 0:
                groups.append(group(mol, kind, [], bonds, kind + 20))
        mol.SetStereoGroups(groups)
        emit(f"mixed groups {sample}", mol, ("chirality", "atrop", "both")[sample % 3])
    for size in range(3, 17):
        for pattern in ("single", "alternating", "aromatic"):
            for stereo in (6, 7):
                for hybrid in (0, 2, 3, 4, 8):
                    for other_hybrid in (0, 3, 4):
                        mol = Chem.RWMol()
                        for _ in range(size):
                            atom = Chem.Atom(6)
                            atom.SetHybridization(Chem.HybridizationType.values[hybrid])
                            mol.AddAtom(atom)
                        for i in range(size):
                            kind = Chem.BondType.SINGLE
                            if pattern == "aromatic":
                                kind = Chem.BondType.AROMATIC
                            elif pattern == "alternating" and i % 2 == 0:
                                kind = Chem.BondType.DOUBLE
                            mol.AddBond(i, (i + 1) % size, kind)
                        mol.GetAtomWithIdx(1).SetHybridization(
                            Chem.HybridizationType.values[other_hybrid]
                        )
                        for index in (0, size // 2):
                            mol.GetBondWithIdx(index).SetStereo(Chem.BondStereo.values[stereo])
                        mol.SetStereoGroups(
                            [
                                group(mol, 0, [0, 1, size - 1], [0], 7),
                                group(mol, 1, [size // 2, 0], [size // 2], 13),
                                group(mol, 2, [], [0, size // 2], 42),
                            ]
                        )
                        emit(
                            f"ring {size}/{pattern}/{stereo}/{hybrid}/{other_hybrid}", mol, "atrop"
                        )
    texts = [
        "C[C@H](F)C[C@@H](Cl)Br |&1:1,4|",
        "C[C@H](F)C[C@@H](Cl)Br |o2:1,4|",
        "[Pt@SP1](Cl)(F)(Br)I",
        "[P@TB1](F)(Cl)(Br)(I)N",
        "[Co@OH1](N)(O)(F)(Cl)(Br)I",
        "[2H][C@]([3H])(F)Cl",
        "Clc1ccccc1-c1ccccc1Br",
        "C12C3C4C1C5C2C3C45",
        "N->[Cu+2]<-N",
    ]
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    texts += [t["smiles"] for t in templates if t.get("smiles")]
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    for i, text in enumerate(texts):
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(f"Invalid reference syntax: {text}")
        emit(text + " raw", mol, "both")
        mol.UpdatePropertyCache(strict=False)
        Chem.SetConjugation(mol)
        Chem.SetHybridization(mol)
        emit(text + " computed", mol, "both")
        if i < 1000:
            mol = Chem.RWMol(mol)
            for atom in mol.GetAtoms():
                atom.SetChiralTag(Chem.ChiralType.values[rng.randrange(9)])
                atom.SetUnsignedProp("_chiralPermutation", rng.randrange(35))
            for bond in mol.GetBonds():
                bond.SetStereo(Chem.BondStereo.values[rng.choice((0, 1, 6, 7))])
            atoms = list(range(mol.GetNumAtoms()))
            bonds = list(range(mol.GetNumBonds()))
            if atoms or bonds:
                mol.SetStereoGroups([group(mol, 1, atoms, bonds, 33)])
            emit(text + " annotated", mol, "both", basis=True)
            rng.shuffle(atoms)
            emit(text + " permuted", Chem.RenumberAtoms(mol, atoms), "both")


if __name__ == "__main__":
    main()
