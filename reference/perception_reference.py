"""Complete legacy stereo assignment from RDKit's API, independent of the worker."""

import json
import random
from itertools import product
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .kekulize_reference import DIRECTIONS
    from .stereo_reference import group, state
else:
    from kekulize_reference import DIRECTIONS
    from stereo_reference import group, state


def optional(obj, name, getter):
    return getter(name) if obj.HasProp(name) else None


def boolean(obj, name):
    return bool(obj.GetPropsAsDict(True, True)[name]) if obj.HasProp(name) else None


def snapshot(mol, ring_kind):
    result = state(mol)
    result["valences"] = [
        dict(
            explicit_valence=a.GetValence(Chem.ValenceType.EXPLICIT),
            implicit_hydrogens=a.GetNumImplicitHs(),
        )
        for a in mol.GetAtoms()
    ]
    result["rings"] = dict(
        kind=ring_kind, atoms=list(mol.GetRingInfo().AtomRings()) if ring_kind != "none" else []
    )
    result["properties"] = dict(
        atoms=[
            dict(
                cip_code=optional(a, "_CIPCode", a.GetProp),
                cip_rank=optional(a, "_CIPRank", a.GetUnsignedProp),
                possible=boolean(a, "_ChiralityPossible"),
                ring_candidate=boolean(a, "_ringStereochemCand"),
                ring_members=list(a.GetPropsAsDict(True, True)["_ringStereoAtoms"])
                if a.HasProp("_ringStereoAtoms")
                else None,
                unknown=bool(a.HasProp("_UnknownStereo") and a.GetIntProp("_UnknownStereo")),
            )
            for a in mol.GetAtoms()
        ],
        bond_codes=[optional(b, "_CIPCode", b.GetProp) for b in mol.GetBonds()],
        done=boolean(mol, "_StereochemDone"),
        needs_detection=boolean(mol, "_needsDetectBondStereo"),
    )
    return result


def emit(name, original, clean=True, force=True, possible=False, rings="symmetric", stale=False):
    mol = Chem.Mol(original)
    if rings == "none":
        mol.ClearComputedProps(includeRings=True)
    elif rings == "fast":
        Chem.FastFindRings(mol)
    elif rings == "basis":
        Chem.GetSSSR(mol)
    else:
        Chem.GetSymmSSSR(mol)
    mol.UpdatePropertyCache(strict=False)
    if stale:
        mol.SetBoolProp("_StereochemDone", False)
        mol.SetBoolProp("_needsDetectBondStereo", True)
        for a in mol.GetAtoms():
            a.SetProp("_CIPCode", ("R", "S", "r", "")[a.GetIdx() % 4])
            a.SetUnsignedProp("_CIPRank", 333 + a.GetIdx())
            a.SetBoolProp("_ChiralityPossible", a.GetIdx() % 2 == 0)
            a.SetBoolProp("_ringStereochemCand", a.GetIdx() % 2 != 0)
        for b in mol.GetBonds():
            b.SetProp("_CIPCode", "stale")
    before = snapshot(mol, rings)
    early = not force and mol.HasProp("_StereochemDone")
    kind = rings if early else ("symmetric" if clean else "fast" if rings == "none" else rings)
    failure = None
    try:
        Chem.AssignStereochemistry(
            mol, cleanIt=clean, force=force, flagPossibleStereoCenters=possible
        )
        expected = snapshot(mol, kind)
    except (ValueError, RuntimeError) as error:
        expected, failure = None, str(error)
    print(
        json.dumps(
            dict(
                name=f"{name}/{clean}/{force}/{possible}/{rings}/{stale}",
                before=before,
                options=dict(clean=clean, force=force, flag_possible=possible),
                expected=expected,
                failure=failure,
            )
        )
    )


def stars():
    for number, tag, degree, hs, charge in product(
        (6, 7, 15, 16, 33, 34, 78), range(9), range(7), (0, 1, 2), (-1, 0, 1)
    ):
        mol = Chem.RWMol()
        atom = Chem.Atom(number)
        atom.SetChiralTag(Chem.ChiralType.values[tag])
        atom.SetNumExplicitHs(hs)
        atom.SetFormalCharge(charge)
        atom.SetNoImplicit(True)
        atom.SetHybridization(Chem.HybridizationType.SP3)
        mol.AddAtom(atom)
        for element in (9, 17, 35, 53, 8, 7)[:degree]:
            a = Chem.Atom(element)
            a.SetNoImplicit(True)
            mol.AddBond(0, mol.AddAtom(a), Chem.BondType.SINGLE)
        emit(f"star {number}/{tag}/{degree}/{hs}/{charge}", mol, possible=True, rings="none")


def alkenes(rng):
    for text in ("FC=CCl", "FC(Cl)=C(Br)I", "CC=C(C)C", "CC=CC=C(C)F", "C=N", "C=O"):
        base = Chem.MolFromSmiles(text)
        for sample in range(300):
            mol = Chem.RWMol(base)
            for a in mol.GetAtoms():
                if rng.random() < 0.15:
                    a.SetIntProp("_UnknownStereo", 1)
            for b in mol.GetBonds():
                b.SetBondDir(rng.choice(list(DIRECTIONS.values())))
                if rng.random() < 0.15:
                    b.SetIntProp("_UnknownStereo", 1)
                if b.GetBondType() == Chem.BondType.DOUBLE:
                    left = [
                        a.GetIdx()
                        for a in b.GetBeginAtom().GetNeighbors()
                        if a.GetIdx() != b.GetEndAtomIdx()
                    ]
                    right = [
                        a.GetIdx()
                        for a in b.GetEndAtom().GetNeighbors()
                        if a.GetIdx() != b.GetBeginAtomIdx()
                    ]
                    if left and right:
                        b.SetStereoAtoms(left[0], right[0])
                    b.SetStereo(Chem.BondStereo.values[rng.randrange(6 if left and right else 4)])
            emit(f"alkene {text}/{sample}", mol, clean=bool(sample % 2), possible=True)
    for size in range(3, 13):
        mol = Chem.RWMol(Chem.MolFromSmiles("C1=" + "C" * (size - 1) + "1"))
        for sample in range(80):
            for b in mol.GetBonds():
                b.SetBondDir(rng.choice(list(DIRECTIONS.values())))
                b.SetStereo(Chem.BondStereo.values[rng.randrange(2)])
            emit(
                f"ring alkene {size}/{sample}",
                mol,
                clean=bool(sample % 2),
                possible=True,
                rings=("none", "fast", "basis", "symmetric")[sample % 4],
            )


def annotated_groups(rng):
    texts = (
        "CC(F)(Cl)C=C(Br)C",
        "C[C@H]1CCC[C@@H](C)C1",
        "C1C2CC3CC1CC(C2)C3",
        "C1CC2CCC1C2",
        "N12CCC(CC1)CC2",
        "C[N@]1CC1",
        "CN1C=CC=C1",
        "C[S@](=O)CC",
        "CCN->[Cu]<-NCC",
    )
    for sample in range(2500):
        mol = Chem.RWMol(Chem.MolFromSmiles(texts[sample % len(texts)]))
        for a in mol.GetAtoms():
            a.SetChiralTag(Chem.ChiralType.values[rng.randrange(9)])
            if sample % 3 == 0:
                a.SetUnsignedProp("_chiralPermutation", rng.choice((0, 1, 3, 20, 31)))
            if sample % 5 == 0:
                a.SetAtomMapNum(rng.randrange(12))
                a.SetIsotope(rng.choice((0, 0, 2, 13, 15)))
        for b in mol.GetBonds():
            b.SetBondDir(rng.choice(list(DIRECTIONS.values())))
            b.SetStereo(Chem.BondStereo.values[rng.choice((0, 1, 6, 7))])
        mol.SetStereoGroups(
            [
                group(
                    mol,
                    k,
                    list(range(k, mol.GetNumAtoms(), 3)),
                    list(range(k, mol.GetNumBonds(), 3)),
                    k + 17,
                )
                for k in range(3)
            ]
        )
        emit(
            f"mixed groups {sample}",
            mol,
            clean=bool(sample % 2),
            possible=True,
            rings=("none", "fast", "basis", "symmetric")[sample % 4],
        )


def main():
    RDLogger.DisableLog("rdApp.*")
    Chem.SetUseLegacyStereoPerception(True)
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    rng = random.Random(191231)
    for clean, force, possible, rings in product(
        (False, True), (False, True), (False, True), ("none", "fast", "basis", "symmetric")
    ):
        emit("empty", Chem.Mol(), clean, force, possible, rings)
    stars()
    alkenes(rng)
    annotated_groups(rng)
    texts = [
        "N[C@@H](C)C(=O)O",
        "C[C@H](O)[C@@H](O)C",
        "F/C=C/F",
        "F/C=C\\F",
        "C[C@H]1CCC[C@@H](C)C1",
        "C[C@H]1CCC[C@H](C)C1",
        "C[C@H]1CCC[C@@H](C)C1 |&1:1,5|",
        "C[C@H]1CCC[C@H](C)C1 |o1:1,5|",
        "C[C@H](O)[C@H](O)C |a:1,3|",
        "C[C@H](O)[C@H](O)C |&1:1,o2:3|",
        "Cl[Pt@SP1](Cl)(N)N",
        "F[P@TB1](Cl)(Br)(I)N",
        "F[Co@OH1](Cl)(Br)(I)(N)O",
        "C[C@H](O)[C@H](F)[C@H](O)C",
        "C[C@H](O)[C@@H](F)[C@H](O)C",
        "C[C@H](O)[C@H](F)[C@@H](O)C",
        "C[C@H]1CC[C@H](C)CC1",
        "C1[C@H]2C[C@H]3C[C@@H]1C[C@@H](C2)C3",
        "C1CC2CCC1C2",
        "C[C@H]1CC[C@H](C)C[C@H]1C",
        "C[C@H]1CC[C@@H](C)C[C@@H]1C",
        "C1C2CC3CC1CC(C2)C3",
        "C12C3C4C1C5C2C3C45",
        "N12CCC(CC1)CC2",
        "C[N@]1CC1",
        "C[S@](=O)CC",
        "C[P@](O)F",
        "[2H][C@H](F)Cl",
        "[13CH3][C@H]([12CH3])F",
        "[Cu]<-NCCN->[Cu]",
        "C[C@H]1CC2CCCC3CCCC(C1)[C@@H]23",
        "C[C@@H]1C[C@H](N2CCc3ccc(N)cc3C2)C[C@H](C)C1",
        "C[C@H]1C[C@H](N2CCc3ccc(N)cc3C2)C[C@H](C)C1",
        "C/C=C\\[C@](/C=C/C)(CC)CO",
        "C/C=C/[C@](/C=C/C)(CC)CO",
        "CC/C=C\\C(\\C=C/CC)=C(CC)CO",
        "CCC(CO)=C([C@H](C)F)[C@@H](C)F",
    ]
    for text in texts:
        base = Chem.MolFromSmiles(text)
        if base is None:
            raise ValueError(text)
        for sample in range(36):
            mol = Chem.RWMol(base)
            if sample % 3 == 0:
                order = list(range(mol.GetNumAtoms()))
                rng.shuffle(order)
                mol = Chem.RWMol(Chem.RenumberAtoms(mol, order))
            if sample % 5 == 0:
                mol = Chem.RWMol(Chem.AddHs(mol))
            if sample % 7 == 0:
                for atom in mol.GetAtoms():
                    atom.SetChiralTag(Chem.ChiralType.values[rng.choice((0, 1, 2))])
                mol.SetStereoGroups(
                    [
                        group(mol, k, list(range(k, mol.GetNumAtoms(), 3)), [], k + 8)
                        for k in range(3)
                    ]
                )
            emit(
                f"stereo {text}/{sample}",
                mol,
                clean=bool(sample % 2),
                force=sample % 9 != 0,
                possible=sample % 3 != 0,
                rings=("none", "fast", "basis", "symmetric")[sample % 4],
                stale=sample % 11 == 0,
            )
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    texts = [t["smiles"] for t in templates if t.get("smiles")]
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    for text in texts:
        mol = Chem.MolFromSmiles(text)
        if mol is None:
            continue
        emit(text, mol, possible=True)
        emit(text + " fast", mol, possible=True, rings="fast")
        emit(text + " retained", mol, clean=False, possible=True, stale=True)
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(text + " permuted", Chem.RenumberAtoms(mol, order), rings="none", possible=True)
        mol = Chem.RWMol(mol)
        for atom in mol.GetAtoms():
            atom.SetChiralTag(Chem.ChiralType.values[rng.choice((0, 1, 2))])
        for bond in mol.GetBonds():
            bond.SetBondDir(rng.choice(list(DIRECTIONS.values())))
        emit(text + " annotated", mol, possible=True)


if __name__ == "__main__":
    main()
