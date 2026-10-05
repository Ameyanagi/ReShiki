"""Native isomeric SMILES from prepared connected components, independent of Rust."""

import json
import random
from itertools import product
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .perception_reference import snapshot
    from .smiles_traversal_reference import molecules
    from .valence_reference import ORDERS
else:
    from perception_reference import snapshot
    from smiles_traversal_reference import molecules
    from valence_reference import ORDERS


def emit(name, original, canonical=True, root=None, explicit=False, isomeric=True):
    mol = Chem.RWMol(original)
    mol.UpdatePropertyCache(strict=False)
    Chem.GetSymmSSSR(mol)
    if not mol.HasProp("_StereochemDone"):
        Chem.AssignStereochemistry(mol, cleanIt=True)
    # Standard SMILES omits enhanced groups and unspecified display directions.
    mol.SetStereoGroups([])
    for bond in mol.GetBonds():
        if bond.GetBondDir() in (Chem.BondDir.UNKNOWN, Chem.BondDir.EITHERDOUBLE):
            bond.SetBondDir(Chem.BondDir.NONE)
        if bond.GetStereo() == Chem.BondStereo.STEREOANY:
            bond.SetStereo(Chem.BondStereo.STEREONONE)
    before = snapshot(mol, "symmetric")
    ranks = (
        list(Chem.CanonicalRankAtoms(mol, includeChirality=isomeric, includeIsotopes=isomeric))
        if canonical
        else list(range(mol.GetNumAtoms()))
    )
    start = root if root is not None else min(range(len(ranks)), key=ranks.__getitem__)
    broken = [bool(a.HasProp("_brokenChirality")) for a in mol.GetAtoms()]
    expected = None
    failure = None
    try:
        text = Chem.MolToSmiles(
            mol,
            isomericSmiles=isomeric,
            canonical=canonical,
            rootedAtAtom=-1 if root is None else root,
            allHsExplicit=explicit,
            allBondsExplicit=explicit,
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
                name=f"{name}/{canonical}/{root}/{explicit}/{isomeric}",
                before=before,
                ranks=ranks,
                start=start,
                broken=broken,
                explicit=explicit,
                isomeric=isomeric,
                ring_bonds=[b.IsInRing() for b in mol.GetBonds()],
                expected=expected,
                failure=failure,
            )
        )
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    rng = random.Random(93573)
    extra = (
        "F[C@H]1CCCC1Cl",
        "C[C@H]1CCC[C@@H](C)C1",
        "C[C@@H]1CC[C@H](C)CC1",
        "F/C(Cl)=C(Br)/I",
        "F/C=C/C=C\\Cl",
        "F/C=C/C=C/C=C\\Cl",
        "CO/C1=C/C=C\\C=C/C=N\\1",
        "C=c1s/c2n(c1=O)CCCCC\\N=2",
        "C[N@]1CC1",
        "N[C@H](C)C(=O)O",
        "C[S@](=O)CC",
        "[P@H](F)Cl",
        "[As@H](F)Cl",
        "[Pt@SP1](Cl)(F)(Br)I",
        "[P@TB1](F)(Cl)(Br)(I)N",
        "[Co@OH1](F)(Cl)(Br)(I)(N)O",
        "S=P(=N\\C)/C",
        "C[N@]1CC1F",
        "C[P@]1CCCC1",
        "C[As@]1CCCC1",
        "C[Se@](=O)CC",
        # Pinned smitest1.cpp ring-direction and natural-product regressions.
        r"C1COC/C=C\CCC1",
        "C1COC/C=C/CCC1",
        "C1CC/C=C/C=C/CCC1",
        "C/1=C/C=C/CCCCCC1",
        "C1COC/C=C/C=C/C1",
        r"C1=C/OCC/C=C\CC\1",
        "C1CCCCN/C=C/1",
        "CCC/[N+]/1=C/c2ccccc2OC(=O)/C=C1/O",
        r"NC(=O)O[C@H]1C(/C)=C/[C@H](C)[C@@H](O)[C@@H](OC)C[C@H](C)C\C2=C(/OC)C(=O)\C=C(\NC(=O)C(\C)=C\C=C/[C@@H]1OC)C2=O",
        r"CC(O[C@@H]1C=C(C)[C@H]2[C@H]([C@H]3O[C@@H]2C/C(C)=C\CC[C@@]3(C)OC(C)=O)[C@H]1C(OC(C)=O)(C)C)=O",
    )
    for name, text in [*molecules(), *((f"stereo/{s}", s) for s in extra)]:
        mol = Chem.MolFromSmiles(text)
        if (
            mol is None
            or any(a.HasQuery() for a in mol.GetAtoms())
            or any(b.HasQuery() or b.GetBondType() not in ORDERS.values() for b in mol.GetBonds())
        ):
            continue
        for j, part in enumerate(Chem.GetMolFrags(mol, asMols=True, sanitizeFrags=False)):
            if not part.GetNumAtoms():
                continue
            for canonical, explicit in product((True, False), (False, True)):
                emit(f"{name}/{j}", part, canonical=canonical, explicit=explicit)
            order = list(range(part.GetNumAtoms()))
            rng.shuffle(order)
            part = Chem.RenumberAtoms(part, order)
            emit(f"{name}/permuted/{j}", part)
            if name.startswith(("stereo/", "template/")):
                for root in range(part.GetNumAtoms()):
                    emit(f"{name}/root/{j}", part, root=root)
    # Coordination permutations, missing ligands and traversal starting points.
    for tag, maximum, limit in ((6, 4, 3), (7, 5, 20), (8, 6, 30)):
        for degree, permutation in product(range(2, maximum + 1), range(limit + 1)):
            mol = Chem.RWMol()
            atom = Chem.Atom(78)
            atom.SetNoImplicit(True)
            atom.SetChiralTag(Chem.ChiralType.values[tag])
            atom.SetUnsignedProp("_chiralPermutation", permutation)
            mol.AddAtom(atom)
            for number in (9, 17, 35, 53, 7, 8)[:degree]:
                mol.AddBond(0, mol.AddAtom(Chem.Atom(number)), Chem.BondType.SINGLE)
            mol.SetBoolProp("_StereochemDone", True)
            for root in range(mol.GetNumAtoms()):
                emit(f"coordination/{tag}/{degree}/{permutation}", mol, root=root)
    for atomic_number, tag, degree, hs, variant in product(
        (0, 6, 7, 15, 16, 33, 34), range(1, 9), range(8), (0, 1), range(4)
    ):
        mol = Chem.RWMol()
        center = Chem.Atom(atomic_number)
        center.SetNoImplicit(True)
        center.SetNumExplicitHs(hs)
        center.SetChiralTag(Chem.ChiralType.values[tag])
        center.SetHybridization(Chem.HybridizationType.SP3)
        center.SetFormalCharge(variant % 2)
        center.SetIsotope(13 if variant == 3 else 0)
        if variant:
            center.SetUnsignedProp(
                "_chiralPermutation", (0, 1, {6: 3, 7: 20, 8: 30}.get(tag, 2) + 1)[variant - 1]
            )
        if variant == 2:
            center.SetBoolProp("_brokenChirality", False)
        mol.AddAtom(center)
        for i, number in enumerate((9, 17, 35, 53, 8, 7, 6)[:degree]):
            atom = Chem.Atom(number)
            atom.SetNoImplicit(True)
            mol.AddBond(
                0,
                mol.AddAtom(atom),
                Chem.BondType.DATIVE if variant == 3 and i == 0 else Chem.BondType.SINGLE,
            )
        mol.SetBoolProp("_StereochemDone", False)
        for root in {0, degree}:
            emit(
                f"raw-center/{atomic_number}/{tag}/{degree}/{hs}/{variant}",
                mol,
                canonical=False,
                root=root,
            )
    patterns = (
        "FC(Cl)=C(Br)I",
        "FC=CC=CC=CC=CCCl",
        "CC=C(C)C=C(C)C=C(C)C",
        "C1=CC=CC=CC=CC=C1",
        "COC1=CC=CC=CC=N1",
        "S=P(=NC)C",
        "FC=C=CCl",
        "C1=CC2=CC=CC=C2C=C1",
        "FC=CC=CC=NCC=CC=CCCl",
        "CC=C(C=C(C)C)C=C(C=C(C)C)C=C(C)C",
    )
    for sample in range(3000):
        mol = Chem.RWMol(Chem.MolFromSmiles(patterns[sample % len(patterns)]))
        Chem.Kekulize(mol, clearAromaticFlags=True)
        # Mutate annotations without re-perceiving stereo, as imported caches
        # can contain conflicting directions and unspecified intervening bonds.
        for bond in mol.GetBonds():
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
                bond.SetStereo(Chem.BondStereo.values[rng.randrange(6 if left and right else 4)])
            else:
                bond.SetBondDir(Chem.BondDir.values[rng.randrange(7)])
                if sample % 7 == 0:
                    bond.SetBondType(
                        rng.choice(
                            (Chem.BondType.SINGLE, Chem.BondType.AROMATIC, Chem.BondType.DATIVE)
                        )
                    )
        mol.SetBoolProp("_StereochemDone", True)
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        mol = Chem.RenumberAtoms(mol, order)
        emit(
            f"double-directions/{sample}",
            mol,
            canonical=bool(sample % 2),
            root=sample % mol.GetNumAtoms(),
            isomeric=sample % 13 != 0,
        )
    for text in ("N12CCC(CC1)CC2", "CN1C=CC=C1", "CN1CC1F", "CP1CCCC1", "C[P]1C=CCC1"):
        base = Chem.MolFromSmiles(text)
        for tag in (1, 2):
            mol = Chem.RWMol(base)
            for atom in mol.GetAtoms():
                if atom.GetAtomicNum() in (7, 15):
                    atom.SetChiralTag(Chem.ChiralType.values[tag])
            mol.SetBoolProp("_StereochemDone", True)
            for root in range(mol.GetNumAtoms()):
                emit(f"ring-centers/{text}/{tag}", mol, root=root)


if __name__ == "__main__":
    main()
