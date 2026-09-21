"""Complete sanitization oracle using native RDKit, independent of the worker."""

import json
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .kekulize_reference import DIRECTIONS, directions
    from .ranking_reference import metadata
    from .stereo_reference import group, state
    from .valence_reference import ORDERS, atom_molecule, graph
else:
    from kekulize_reference import DIRECTIONS, directions
    from ranking_reference import metadata
    from stereo_reference import group, state
    from valence_reference import ORDERS, atom_molecule, graph


def emit(name, original):
    mol = Chem.Mol(original)
    # The migrated graph carries structural annotations, not computed properties.
    mol.ClearComputedProps(includeRings=True)
    source = dict(graph=graph(mol), metadata=metadata(mol), directions=directions(mol))
    failure = None
    try:
        failed = Chem.SanitizeMol(mol, catchErrors=True)
        if failed:
            expected = None
            failure = str(failed)
        else:
            expected = state(mol)
            expected["rings"] = list(mol.GetRingInfo().AtomRings())
            expected["valences"] = [
                dict(
                    explicit_valence=a.GetValence(Chem.ValenceType.EXPLICIT),
                    implicit_hydrogens=a.GetNumImplicitHs(),
                )
                for a in mol.GetAtoms()
            ]
    except (ValueError, RuntimeError) as error:
        expected = None
        failure = str(error)
    print(json.dumps(dict(name=name, **source, expected=expected, failure=failure)))


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", Chem.Mol())
    for number in range(119):
        for charge in (-2, -1, 0, 1, 2):
            for hydrogens in (0, 1, 2, 4):
                for no_implicit in (False, True):
                    emit(
                        f"atom {number}/{charge}/{hydrogens}/{no_implicit}",
                        atom_molecule(number, charge, hydrogens, 0, no_implicit),
                    )
    texts = [
        "c1ccccc1",
        "[nH]1cccc1",
        "[pH]1cccc1",
        "[n]1cccc1",
        "[13cH]1ccccc1",
        "C[Si](C)(C)C",
        "CN(=O)=O",
        "CN=N#N",
        "O=P(C)(C)=O",
        "O=Cl(=O)(=O)O",
        "N[Cu](N)(N)N",
        "C1=CC=CN1[Cu]",
        "C/C=C/c1ccccc1",
        "C[C@H](N)C(=O)O",
        "N[Pt@SP1](Cl)(F)C",
        "N[P@TB1](Cl)(F)(Br)C",
        "C12C3C4C1C5C2C3C45",
        "c",
        "cc",
        "c1cccc1",
        "*1cccc1",
        "*1:*:*:*:*:*:1",
        "[C-3].[O+2].[2H]",
        # Native #8403/#8606 regressions needing backtracking/canonical retry.
        "c1cc2ccc3c4c(ccc(c1)c24)c1c2c4ccc5cccc6ccc(c4c65)c4c5cccc6c7cccc8c9cc"
        "cc%10c%11cccc%12c3c1c1c(c%11%12)c(c9%10)c(c87)c(c65)c1c42",
        "O=c1c2c3c(c4c(c2c(=O)c2c5c(c6c(c12)c1c2c6cccc2ccc1)c1c2c5cccc2ccc1)c1"
        "c2c4cccc2ccc1)c1c2c3cccc2ccc1",
    ]
    rng = random.Random(56143)
    for text in texts:
        base = Chem.MolFromSmiles(text, sanitize=False)
        if base is None:
            raise ValueError(text)
        for sample in range(30):
            mol = Chem.RWMol(base)
            for atom in mol.GetAtoms():
                if sample % 4 == 0:
                    atom.SetAtomMapNum(rng.randrange(30))
                if sample % 5 == 0:
                    atom.SetChiralTag(Chem.ChiralType.values[rng.randrange(9)])
                    atom.SetUnsignedProp("_chiralPermutation", rng.choice((0, 1, 3, 20, 31)))
            for bond in mol.GetBonds():
                if sample % 3 == 0:
                    bond.SetBondDir(rng.choice(list(DIRECTIONS.values())))
                if sample % 7 == 0:
                    bond.SetStereo(Chem.BondStereo.values[rng.choice((0, 1, 6, 7))])
            if sample % 5 == 0:
                mol.SetStereoGroups(
                    [
                        group(
                            mol,
                            kind,
                            list(range(kind, mol.GetNumAtoms(), 3)),
                            list(range(kind, mol.GetNumBonds(), 3)),
                            kind + 4,
                        )
                        for kind in range(min(3, max(mol.GetNumAtoms(), mol.GetNumBonds())))
                    ]
                )
            order = list(range(mol.GetNumAtoms()))
            rng.shuffle(order)
            emit(f"special {text}/{sample}", Chem.RenumberAtoms(mol, order))
    for sample in range(2000):
        mol = Chem.RWMol(Chem.MolFromSmiles(rng.choice(texts[:15]), sanitize=False))
        for atom in mol.GetAtoms():
            if rng.random() < 0.3:
                atom.SetAtomicNum(rng.choice((0, 5, 6, 7, 8, 15, 16, 26, 29)))
                atom.SetFormalCharge(rng.choice((-1, 0, 1)))
                atom.SetNumExplicitHs(rng.randrange(3))
                atom.SetNoImplicit(rng.random() < 0.5)
                atom.SetNumRadicalElectrons(rng.randrange(3))
                atom.SetIsAromatic(rng.random() < 0.5)
        for bond in mol.GetBonds():
            if rng.random() < 0.3:
                bond.SetBondType(rng.choice(list(ORDERS.values())))
                bond.SetIsAromatic(rng.random() < 0.5)
                bond.SetBondDir(rng.choice(list(DIRECTIONS.values())))
        emit(f"mixed {sample}", mol)
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    texts += [t["smiles"] for t in templates if t.get("smiles")]
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    for text in texts:
        mol = Chem.MolFromSmiles(text, sanitize=False)
        if mol is None:
            raise ValueError(text)
        emit(text + " raw", mol)
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(text + " permuted", Chem.RenumberAtoms(mol, order))
        try:
            Chem.SanitizeMol(mol)
        except (ValueError, RuntimeError):
            continue
        emit(text + " sanitized", mol)
        emit(text + " graph H", Chem.AddHs(mol))
        Chem.Kekulize(mol, clearAromaticFlags=True)
        emit(text + " kekule", mol)


if __name__ == "__main__":
    main()
