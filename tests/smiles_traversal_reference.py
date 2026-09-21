"""Complete native SMILES output with stereo removed to isolate graph traversal."""

import json
import os
import random
from itertools import product
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .ranking_reference import metadata
    from .valence_reference import ORDERS, graph
else:
    from ranking_reference import metadata
    from valence_reference import ORDERS, graph


def emit(name, original, canonical=True, root=None, hydrogens=False, bonds=False):
    mol = Chem.Mol(original)
    Chem.RemoveStereochemistry(mol)
    mol.UpdatePropertyCache(strict=False)
    Chem.GetSymmSSSR(mol)
    source = graph(mol)
    meta = metadata(mol)
    rings = list(mol.GetRingInfo().AtomRings())
    ranks = (
        list(Chem.CanonicalRankAtoms(mol, includeChirality=False, includeIsotopes=False))
        if canonical
        else list(range(mol.GetNumAtoms()))
    )
    start = root if root is not None else min(range(len(ranks)), key=ranks.__getitem__)
    cache = [
        dict(
            explicit_valence=a.GetValence(Chem.ValenceType.EXPLICIT),
            implicit_hydrogens=a.GetNumImplicitHs(),
        )
        for a in mol.GetAtoms()
    ]
    props = [
        dict(
            custom_symbol=a.GetProp("smilesSymbol") if a.HasProp("smilesSymbol") else None,
            supplement=a.GetProp("_supplementalSmilesLabel")
            if a.HasProp("_supplementalSmilesLabel")
            else None,
        )
        for a in mol.GetAtoms()
    ]
    expected = None
    failure = None
    try:
        text = Chem.MolToSmiles(
            mol,
            isomericSmiles=False,
            canonical=canonical,
            rootedAtAtom=-1 if root is None else root,
            allHsExplicit=hydrogens,
            allBondsExplicit=bonds,
        )
        expected = dict(
            text=text,
            atom_order=json.loads(mol.GetProp("_smilesAtomOutputOrder")),
            bond_order=json.loads(mol.GetProp("_smilesBondOutputOrder")),
        )
    except (ValueError, RuntimeError) as error:
        failure = str(error).splitlines()[0]
    print(
        json.dumps(
            dict(
                name=f"{name}/{canonical}/{root}/{hydrogens}/{bonds}",
                graph=source,
                metadata=meta,
                rings=rings,
                ring_bonds=[
                    bool(mol.GetRingInfo().NumBondRings(i)) for i in range(mol.GetNumBonds())
                ],
                ranks=ranks,
                start=start,
                cache=cache,
                properties=props,
                canonical=canonical,
                options=dict(
                    isomeric=False,
                    kekule=False,
                    all_hydrogens=hydrogens,
                    all_bonds=bonds,
                    non_tetrahedral=True,
                ),
                expected=expected,
                failure=failure,
            )
        )
    )


def molecules():
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    for template in templates:
        if text := template.get("smiles"):
            yield f"template/{template['name']}", text
    for i, line in enumerate(
        (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ):
        yield f"nci/{i}", line.split()[0]
    if source := os.environ.get("RESHIKI_RDKIT_SOURCE"):
        directory = Path(source) / "Code/GraphMol/SmilesParse/test_data"
        for path in sorted([*directory.glob("*.smi"), *directory.glob("*.cxsmi")]):
            for i, line in enumerate(path.read_text().splitlines()):
                fields = line.split()
                if fields:
                    yield f"source/{path.name}/{i}", fields[0]


def synthetic():
    # Dense dummy graphs exercise label reuse, %NN, %(NNN), and the native
    # 1,024 simultaneously-open-ring limit without artificial valence errors.
    for size in (3, 6, 12, 24, 48, 64, 66, 68, 70):
        mol = Chem.RWMol()
        for _ in range(size):
            mol.AddAtom(Chem.Atom(0))
        for a in range(size):
            for b in range(a + 1, size):
                mol.AddBond(a, b, Chem.BondType.SINGLE)
        yield f"complete/{size}", mol
    for size, order, aromatic in product(range(3, 10), range(8), (False, True)):
        mol = Chem.RWMol()
        for _ in range(size + 2):
            a = Chem.Atom(0)
            a.SetIsAromatic(aromatic)
            mol.AddAtom(a)
        for a in range(size):
            mol.AddBond(a, (a + 1) % size, ORDERS[order])
        mol.AddBond(0, size, Chem.BondType.SINGLE)
        mol.AddBond(1, size + 1, Chem.BondType.DOUBLE)
        yield f"special-bonds/{size}/{order}/{aromatic}", mol
    # Connect rings through both dative orientations and multiple ring closures.
    for text in (
        "N->1CC[Cu]1CC",
        "C1CC11CC1CC",
        "C12CC2CC1CC",
        "C1.C1CC",
        "C2C1C3CC1CC2C3",
        "C1=CC=CC=C1C2=CC=CC=C2",
        "[13CH3:0][OH:7]",
        "C/C=C/C=C\\C",
        "F[C@H]1CCCC1Cl",
        "C1CCC2C3CCC4CCCC(C1)C234",
        "c1ccccc1-c1ccccc1",
        "[Fe]12345678(N->1)(N->2)(N->3)(N->4)(N->5)(N->6)(N->7)N->8",
    ):
        mol = Chem.MolFromSmiles(text)
        if mol is not None:
            yield text, mol


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    rng = random.Random(39103)
    for name, text in molecules():
        mol = Chem.MolFromSmiles(text)
        if mol is None:
            continue
        # Query and unsupported bond types are outside the editable Graph
        # contract and are tested by the readers' rejection suites.
        if any(a.HasQuery() for a in mol.GetAtoms()) or any(
            b.HasQuery() or b.GetBondType() not in ORDERS.values() for b in mol.GetBonds()
        ):
            continue
        for j, fragment in enumerate(Chem.GetMolFrags(mol, asMols=True, sanitizeFrags=False)):
            if not fragment.GetNumAtoms():
                continue
            for canonical, explicit in product((True, False), (False, True)):
                emit(f"{name}/part/{j}", fragment, canonical, hydrogens=explicit, bonds=explicit)
            order = list(range(fragment.GetNumAtoms()))
            rng.shuffle(order)
            shuffled = Chem.RenumberAtoms(fragment, order)
            emit(f"{name}/permuted/{j}", shuffled)
            if name.startswith("template/"):
                for root in range(fragment.GetNumAtoms()):
                    emit(f"{name}/root/{j}", shuffled, root=root)
    for name, mol in synthetic():
        for canonical, explicit in product((True, False), (False, True)):
            emit(name, mol, canonical, hydrogens=explicit, bonds=explicit)
        if mol.GetNumAtoms() < 20:
            for root in range(mol.GetNumAtoms()):
                emit(name, mol, root=root)
            for i, atom in enumerate(mol.GetAtoms()):
                atom.SetAtomMapNum(i % 3)
                atom.SetIsotope(i % 2)
                if i % 5 == 0:
                    atom.SetProp("smilesSymbol", "R")
                    Chem.SetSupplementalSmilesLabel(atom, "(X)")
            emit(name + "/labels", mol)
    # Canon.cpp offsets gray-neighbor ranks by bond type in steps of 5,000.
    # More than 5,000 atoms can therefore give two distinct ranks the same
    # traversal key. A shallow graph avoids native recursive stack limits.
    for branches in (14, 18, 24, 40, 64, 128, 129, 160, 200):
        base = Chem.RWMol()
        size = 5000 + branches
        for _ in range(size):
            base.AddAtom(Chem.Atom(0))
        for leaf in range(1, 5000):
            base.AddBond((leaf - 1) // 2, leaf, Chem.BondType.SINGLE)
        path = [0, *range(5000, size)]
        for a, b in zip(path, path[1:]):
            base.AddBond(a, b, Chem.BondType.TRIPLE)
        for variant in range(5):
            mol = Chem.RWMol(base)
            previous = path[:-2]
            if variant == 1:
                previous.reverse()
            elif variant > 1:
                rng.shuffle(previous)
            for a in previous:
                # Zero-valence hydrogen bonds allow large candidate lists
                # without overflowing the reference's narrow valence cache.
                # The two ordinary closures still have colliding sort keys.
                order = Chem.BondType.DOUBLE
                if a == 0:
                    order = Chem.BondType.SINGLE
                elif branches > 40 and a != 5000:
                    order = Chem.BondType.HYDROGEN
                mol.AddBond(a, size - 1, order)
            for explicit in (False, True):
                emit(f"wide-closures/{branches}/{variant}", mol, canonical=False, bonds=explicit)


if __name__ == "__main__":
    main()
