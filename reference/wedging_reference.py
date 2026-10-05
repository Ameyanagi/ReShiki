"""Independent native wedge selection, orientation and single-bond geometry."""

import json
import math
import random
from itertools import product
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase
from rdkit.Chem import rdDepictor

if TYPE_CHECKING or __package__:
    from .drawn_stereo_reference import conformer
    from .kekulize_reference import DIRECTIONS, directions
    from .ranking_reference import metadata
    from .valence_reference import graph
else:
    from drawn_stereo_reference import conformer
    from kekulize_reference import DIRECTIONS, directions
    from ranking_reference import metadata
    from valence_reference import graph


def snapshot(mol, kind):
    return dict(
        graph=graph(mol),
        metadata=metadata(mol),
        directions=directions(mol),
        rings=dict(kind=kind, atoms=[] if kind == "none" else list(mol.GetRingInfo().AtomRings())),
    )


def cycle_edges(rings):
    """Compare cached cycles without their start atom, direction or list order."""
    return sorted(
        tuple(sorted((min(a, b), max(a, b)) for a, b in zip(ring, ring[1:] + ring[:1])))
        for ring in rings["atoms"]
    )


def tied_variants(original, kind, parameters, expected):
    """Native outputs under equal-priority atom permutations, not a wedge oracle rewrite.

    std::sort does not specify the order of equal-priority centers. Keep bond
    indices, geometry, winding and all unequal atom priorities fixed. Each
    alternative comes from WedgeMolBonds itself, then is renumbered back.
    """
    if any(int(b.GetStereo()) in (6, 7) for b in original.GetBonds()):
        return []
    tetra = (Chem.ChiralType.CHI_TETRAHEDRAL_CW, Chem.ChiralType.CHI_TETRAHEDRAL_CCW)
    scores = [100] * original.GetNumAtoms()
    for b in original.GetBonds():
        if b.GetBondDir() in (
            Chem.BondDir.BEGINWEDGE,
            Chem.BondDir.BEGINDASH,
            Chem.BondDir.UNKNOWN,
        ):
            for a in (b.GetBeginAtom(), b.GetEndAtom()):
                if a.GetChiralTag() in tetra:
                    scores[a.GetIdx()] = 101
                    break
    for a in original.GetAtoms():
        if scores[a.GetIdx()] != 101 and a.GetChiralTag() in tetra:
            scores[a.GetIdx()] = -sum(
                10 if n.GetAtomicNum() == 1 else int(n.GetChiralTag() in tetra)
                for n in a.GetNeighbors()
            )
    if not any(
        b.GetBondType() == Chem.BondType.SINGLE
        and scores[b.GetBeginAtomIdx()] == scores[b.GetEndAtomIdx()] < 100
        for b in original.GetBonds()
    ):
        return []
    base = Chem.Mol(original)
    if kind in ("none", "fast"):
        Chem.GetSSSR(base)
        kind = "basis"
    groups = {}
    for i, score in enumerate(scores):
        if score < 100:
            groups.setdefault(score, []).append(i)
    rng = random.Random(27193)
    seen = {json.dumps(expected, sort_keys=True)}
    variants = []
    for _ in range(64):
        order = list(range(len(scores)))
        for group in groups.values():
            shuffled = list(group)
            rng.shuffle(shuffled)
            for position, atom in zip(group, shuffled, strict=True):
                order[position] = atom
        mol = Chem.RenumberAtoms(base, order)
        inverse = [0] * len(order)
        for i, old in enumerate(order):
            inverse[old] = i
        try:
            Chem.WedgeMolBonds(mol, mol.GetConformer(), parameters)
        except (ValueError, RuntimeError):
            continue
        restored = Chem.RenumberAtoms(mol, inverse)
        alternative = snapshot(restored, kind)
        # RenumberAtoms can change the ring-cache initialization level, causing
        # WedgeMolBonds to rebuild the same cycles in a different order. Verify
        # cycle identity before restoring the unpermuted reference cache. The
        # Rust result must still preserve that exact original cache.
        if alternative["rings"]["kind"] != expected["rings"]["kind"] or cycle_edges(
            alternative["rings"]
        ) != cycle_edges(expected["rings"]):
            continue
        alternative["rings"] = expected["rings"]
        key = json.dumps(alternative, sort_keys=True)
        if key not in seen:
            seen.add(key)
            variants.append(alternative)
    return variants


def emit(name, original, two=False, single=None, kind="symmetric"):
    mol = Chem.Mol(original)
    if kind == "none":
        mol.ClearComputedProps(includeRings=True)
    elif kind == "fast":
        Chem.FastFindRings(mol)
    elif kind == "basis":
        Chem.GetSSSR(mol)
    else:
        Chem.GetSymmSSSR(mol)
    mol.UpdatePropertyCache(strict=False)
    before = snapshot(mol, kind)
    props = dict(
        valences=[
            dict(
                explicit_valence=a.GetValence(Chem.ValenceType.EXPLICIT),
                implicit_hydrogens=a.GetNumImplicitHs(),
            )
            for a in mol.GetAtoms()
        ],
        attachment_points=[bool(a.HasProp("_fromAttchpt")) for a in mol.GetAtoms()],
    )
    conf = mol.GetConformer() if mol.GetNumConformers() else None
    coords = (
        None
        if conf is None
        else dict(
            is_3d=conf.Is3D(),
            positions=[
                dict(x=p.x, y=p.y, z=p.z)
                for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
            ],
        )
    )
    failure = None
    unwedged = Chem.Mol(mol)
    alternatives = []
    try:
        if single is None:
            parameters = Chem.BondWedgingParameters()
            parameters.wedgeTwoBondsIfPossible = two
            Chem.WedgeMolBonds(mol, conf, parameters)
            after_kind = "basis" if kind in ("none", "fast") else kind
        else:
            Chem.WedgeBond(mol.GetBondWithIdx(single[0]), single[1], conf)
            after_kind = kind
        expected = snapshot(mol, after_kind)
    except (ValueError, RuntimeError) as error:
        expected = None
        failure = str(error)
    if expected is not None and single is None:
        alternatives = tied_variants(unwedged, kind, parameters, expected)
    print(
        json.dumps(
            dict(
                name=f"{name}/{two}/{single}/{kind}",
                before=before,
                properties=props,
                conformer=coords,
                two=two,
                single=single,
                expected=expected,
                alternatives=alternatives,
                failure=failure,
            )
        )
    )


def stars(rng):
    points = [
        (0, 0, 0),
        (1, 0, 0),
        (-0.5, 0.866, 0),
        (-0.5, -0.866, 0),
        (0, 1, 1),
        (0, -1, -1),
        (2, 1, 0),
    ]
    for degree, tag, reverse in product(range(7), (1, 2), (False, True)):
        mol = Chem.RWMol()
        for i in range(degree + 1):
            a = Chem.Atom((6, 9, 17, 35, 1, 8, 7)[i])
            a.SetNoImplicit(True)
            mol.AddAtom(a)
            if i:
                mol.AddBond(i if reverse else 0, 0 if reverse else i, Chem.BondType.SINGLE)
        mol.GetAtomWithIdx(0).SetChiralTag(Chem.ChiralType.values[tag])
        conformer(mol, points[: degree + 1])
        for two, kind in product((False, True), ("none", "fast", "basis", "symmetric")):
            emit(f"degree {degree}/{tag}/{reverse}", mol, two=two, kind=kind)
        for b in mol.GetBonds():
            emit(f"single {degree}/{tag}/{reverse}/{b.GetIdx()}", mol, single=(b.GetIdx(), 0))
    for sample in range(1800):
        mol = Chem.RWMol(Chem.MolFromSmiles("F[C@](Cl)(Br)I"))
        mol.GetAtomWithIdx(1).SetChiralTag(Chem.ChiralType.values[1 + sample % 2])
        for a in mol.GetAtoms():
            if rng.random() < 0.3:
                a.SetIntProp("_fromAttchpt", rng.choice((0, 1)))
        for b in mol.GetBonds():
            b.SetBondDir(rng.choice(list(DIRECTIONS.values())))
        points = [
            (math.cos(v), math.sin(v), rng.uniform(-2, 2))
            for v in [rng.uniform(-math.pi, math.pi) for _ in range(5)]
        ]
        points[1] = (0, 0, 0)
        if sample % 20 == 0:
            points[0] = points[1]
        elif sample % 20 == 1:
            points = [(float(i), 0, 0) for i in range(5)]
        elif sample % 20 == 2:
            points = [(1e-20 * p[0], 1e-20 * p[1], 0) for p in points]
        conformer(mol, points)
        emit(f"star coordinates {sample}", mol, two=sample % 2 == 0)
        emit(f"single coordinates {sample}", mol, single=(sample % 4, 1))
    for degrees in (1, 1.89, 1.9, 1.91, 2, 60, 90, 119.99, 120, 120.01, 178.09, 178.1, 178.11, 180):
        for sign, tag in product((-1, 1), (1, 2)):
            mol = Chem.RWMol(Chem.MolFromSmiles("F[C@H](Cl)Br"))
            mol.GetAtomWithIdx(1).SetChiralTag(Chem.ChiralType.values[tag])
            a = math.radians(degrees)
            conformer(mol, [(1, 0, 0), (0, 0, 0), (sign * math.cos(a), math.sin(a), 0), (-1, 0, 0)])
            emit(f"boundary {degrees}/{sign}/{tag}", mol)


def atropisomers(rng):
    for text in ("FC(Cl)C(Br)I", "Fc1cccc(Cl)c1-c1c(Br)cccc1I", "C1CCCCC1C1CCCCC1"):
        base = Chem.MolFromSmiles(text)
        rdDepictor.Compute2DCoords(base)
        for sample in range(600):
            mol = Chem.RWMol(base)
            for a in mol.GetAtoms():
                a.SetNoImplicit(True)
                if sample % 6 == 0 and a.GetDegree() == 3:
                    a.SetChiralTag(Chem.ChiralType.values[rng.choice((0, 1, 2))])
            for b in mol.GetBonds():
                if (
                    b.GetBondType() == Chem.BondType.SINGLE
                    and b.GetBeginAtom().GetDegree() in (2, 3)
                    and b.GetEndAtom().GetDegree() in (2, 3)
                ):
                    b.SetStereo(Chem.BondStereo.values[6 + sample % 2])
                if sample % 3 == 0:
                    b.SetBondDir(rng.choice(list(DIRECTIONS.values())))
            conf = mol.GetConformer()
            conf.Set3D(sample % 2 == 0)
            for i in range(mol.GetNumAtoms()):
                p = conf.GetAtomPosition(i)
                conf.SetAtomPosition(
                    i,
                    (
                        -p.x if sample % 4 == 0 else p.x,
                        p.y,
                        rng.uniform(-1, 1) if conf.Is3D() else 0,
                    ),
                )
            if sample % 10 == 0:
                conf.SetAtomPosition(1, conf.GetAtomPosition(0))
            emit(
                f"atrop {text}/{sample}",
                mol,
                two=sample % 3 == 1,
                kind=("none", "fast", "basis", "symmetric")[sample % 4],
            )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    rng = random.Random(191233)
    mol = Chem.Mol()
    emit("empty missing conformer", mol)
    conformer(mol, [])
    emit("empty", mol)
    stars(rng)
    atropisomers(rng)
    texts = [
        "C[C@H](O)[C@@H](O)C",
        "C[C@H]1CCC[C@H](C)C1",
        "C1C2CC3CC1CC(C2)C3",
        "F[C@](Cl)(Br)[C@@](F)(Cl)I",
        "[H][C@](C)(N)O",
        "F[C@](O)(N)C(C)=C/C",
        "C[C@H]1CC2CCCC3CCCC(C1)[C@@H]23",
    ]
    texts += [
        t["smiles"]
        for t in json.loads(
            (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
        )
        if t.get("smiles")
    ]
    texts += [
        line.split()[0]
        for line in (Path(RDConfig.RDDataDir) / "NCI/first_5K.smi").read_text().splitlines()
    ]
    for text in texts:
        mol = Chem.MolFromSmiles(text)
        if mol is None:
            continue
        rdDepictor.Compute2DCoords(mol)
        emit(text, mol)
        emit(text + " two", mol, two=True, kind="none")
        order = list(range(mol.GetNumAtoms()))
        rng.shuffle(order)
        emit(text + " permuted", Chem.RenumberAtoms(mol, order), two=True, kind="basis")
        for a in mol.GetAtoms():
            if a.GetDegree() in (3, 4):
                a.SetChiralTag(Chem.ChiralType.values[rng.choice((0, 1, 2))])
        emit(text + " annotated", mol, two=True)


if __name__ == "__main__":
    main()
