"""Native default RemoveHs with explicit-count updates, independent of Rust."""

import json
import random
from itertools import product
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .kekulize_reference import directions
    from .ranking_reference import metadata
    from .valence_reference import graph
else:
    from kekulize_reference import directions
    from ranking_reference import metadata
    from valence_reference import graph


def snapshot(mol):
    return dict(
        graph=graph(mol),
        metadata=metadata(mol),
        directions=directions(mol),
        unknown_atoms=[
            bool(a.HasProp("_UnknownStereo") and a.GetIntProp("_UnknownStereo"))
            for a in mol.GetAtoms()
        ],
    )


def emit(name, original, protected=(), groups=()):
    mol = Chem.Mol(original)
    for atom in mol.GetAtoms():
        atom.SetUnsignedProp("removal_reference_id", atom.GetIdx())
    for bond in mol.GetBonds():
        bond.SetUnsignedProp("removal_reference_id", bond.GetIdx())
    before = snapshot(mol)
    before["annotations"] = dict(protected_atoms=list(protected), substance_groups=list(groups))
    try:
        params = Chem.RemoveHsParameters()
        params.updateExplicitCount = True
        mol = Chem.RemoveHs(mol, params, sanitize=False)
        expected = snapshot(mol)
        expected["kept_atoms"] = [a.GetUnsignedProp("removal_reference_id") for a in mol.GetAtoms()]
        expected["kept_bonds"] = [b.GetUnsignedProp("removal_reference_id") for b in mol.GetBonds()]
        failure = None
    except (ValueError, RuntimeError, OverflowError) as error:
        expected, failure = None, str(error)
    print(json.dumps(dict(name=name, before=before, expected=expected, failure=failure)))


def cases():
    rng = random.Random(492372)
    params = Chem.SmilesParserParams()
    params.sanitize = False
    params.removeHs = False
    for number in (119, 127, 255):
        mol = Chem.RWMol()
        mol.AddAtom(Chem.Atom(number))
        emit(f"invalid element/{number}", mol)
    for text in (
        "",
        "[H]",
        "[H][H]",
        "[H]*",
        "[H]C",
        "[H+]C",
        "[H-]C",
        "[1H]C",
        "[2H]O[3H]",
        "[H:7]C",
        "[H]C([H])([H])[H]",
        "[H][C@](F)(Cl)Br",
        "F[C@]([H])(Cl)Br",
        "F[C@](Cl)(Br)[H]",
        "[H]/N=C/F",
        "F/C([H])=C([H])/Cl",
        "[H]/C(F)=C(Cl)/[H]",
        "[H]1[H]CC1",
        "[H]C1CC[H]1",
        "[H]->[Cu]",
        "[Cu]->[H]",
        "[H]$[Mo]",
        "[H]=C",
        "[H][n+]1ccccc1",
        "[H][Pt@SP1](F)(Cl)Br",
        "[H][C@AL1](F)Cl",
        "[H][C@TH2](F)(Cl)Br",
    ):
        mol = Chem.MolFromSmiles(text, params)
        if mol is not None:
            for reverse in (False, True):
                emit(
                    f"SMILES/{text}/{reverse}",
                    Chem.RenumberAtoms(mol, list(reversed(range(mol.GetNumAtoms()))))
                    if reverse and mol.GetNumAtoms()
                    else mol,
                )
    # Vary hydrogen identity, neighbor class, degree and stereo declarations.
    for number, hs, tag, hydrogen, order in product(
        (0, 1, 6, 7, 8, 15, 16, 78),
        range(1, 5),
        (0, 1, 2, 4, 5, 6, 7, 8),
        ((0, 0), (2, 0), (0, -1), (0, 1)),
        (Chem.BondType.SINGLE, Chem.BondType.DATIVE),
    ):
        mol = Chem.RWMol()
        atom = Chem.Atom(number)
        atom.SetChiralTag(Chem.ChiralType.values[tag])
        atom.SetNoImplicit(bool(hs % 2))
        atom.SetNumExplicitHs(255 if hs == 4 else hs % 2)
        if tag >= 4:
            atom.SetUnsignedProp("_chiralPermutation", hs % 3)
        mol.AddAtom(atom)
        for index in range(hs):
            atom = Chem.Atom(1)
            atom.SetIsotope(hydrogen[0])
            atom.SetFormalCharge(hydrogen[1])
            atom.SetNoImplicit(True)
            atom.SetAtomMapNum(index + 1)
            mol.AddBond(0, mol.AddAtom(atom), order)
        for number2 in (9, 17)[: hs % 3]:
            mol.AddBond(0, mol.AddAtom(Chem.Atom(number2)), Chem.BondType.SINGLE)
        order_atoms = list(range(mol.GetNumAtoms()))
        rng.shuffle(order_atoms)
        emit(f"star/{number}/{hs}/{tag}/{hydrogen}/{order}", Chem.RenumberAtoms(mol, order_atoms))
    for text in ("[H]C(F)=C(Cl)[H]", "[H]N=C(F)Cl", "[H]C([H])=C([H])[H]", "[H]C(F)(Cl)Br"):
        original = Chem.MolFromSmiles(text, params)
        if original is None:
            raise RuntimeError("Invalid hydrogen-removal fixture")
        for sample in range(300):
            mol = Chem.RWMol(original)
            for atom in mol.GetAtoms():
                if atom.GetAtomicNum() > 1:
                    atom.SetChiralTag(Chem.ChiralType.values[rng.randrange(3)])
            for bond in mol.GetBonds():
                if bond.GetBondType() == Chem.BondType.SINGLE:
                    bond.SetBondDir(Chem.BondDir.values[rng.randrange(7)])
                if bond.GetBondType() == Chem.BondType.DOUBLE:
                    left = [
                        a.GetIdx()
                        for a in bond.GetBeginAtom().GetNeighbors()
                        if a.GetIdx() != bond.GetEndAtomIdx()
                    ]
                    right = [
                        a.GetIdx()
                        for a in bond.GetEndAtom().GetNeighbors()
                        if a.GetIdx() != bond.GetBeginAtomIdx()
                    ]
                    if left and right:
                        bond.SetStereoAtoms(rng.choice(left), rng.choice(right))
                        bond.SetStereo(Chem.BondStereo.values[rng.randrange(6)])
            atom_order = list(range(mol.GetNumAtoms()))
            rng.shuffle(atom_order)
            emit(f"directions/{text}/{sample}", Chem.RenumberAtoms(mol, atom_order))
    source = Chem.SDMolSupplier(
        str(Path(RDConfig.RDDataDir) / "NCI/first_200.props.sdf"), removeHs=False
    )
    for index, molecule in enumerate(source):
        if molecule is None:
            continue
        molecule = Chem.AddHs(molecule)
        for sample in range(3):
            order_atoms = list(range(molecule.GetNumAtoms()))
            if sample:
                rng.shuffle(order_atoms)
            emit(f"NCI/{index}/{sample}", Chem.RenumberAtoms(molecule, order_atoms))
    # Substance groups retain hydrogens when removal would empty the group.
    for members in ([0], [2], [0, 2], [0, 1], [0, 1, 2]):
        mol = Chem.RWMol(Chem.MolFromSmiles("[H]C[H]", params))
        group = Chem.CreateMolDataSubstanceGroup(mol, "field", "value")
        group.SetAtoms(members)
        emit(f"group/{members}", mol, groups=[members])
    # Attachment atoms have protected roles even inside a larger group.
    for index in (0, 2):
        mol = Chem.RWMol(Chem.MolFromSmiles("[H]C[H]", params))
        group = Chem.CreateMolSubstanceGroup(mol, "SUP")
        group.SetAtoms([0, 1, 2])
        group.AddAttachPoint(index, -1, "1")
        emit(f"attachment/{index}", mol, protected=[index], groups=[[0, 1, 2]])
    for memberships in ([[0], [0, 2]], [[0, 2], [0]], [[0, 1], [0, 2]], [[2], [0]]):
        mol = Chem.RWMol(Chem.MolFromSmiles("[H]C[H]", params))
        for members in memberships:
            group = Chem.CreateMolDataSubstanceGroup(mol, "field", "value")
            group.SetAtoms(members)
        emit(f"overlapping groups/{memberships}", mol, groups=memberships)
    for kind, atoms, bonds in product(range(3), ([0], [1], [0, 1, 2]), ([], [0], [0, 1])):
        mol = Chem.RWMol(Chem.MolFromSmiles("[H]C[H]", params))
        group = Chem.CreateStereoGroup(
            Chem.StereoGroupType.values[kind], mol, atomIds=atoms, bondIds=bonds, readId=7
        )
        group.SetWriteId(17)
        mol.SetStereoGroups([group])
        emit(f"stereo group/{kind}/{atoms}/{bonds}", mol)


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    cases()
