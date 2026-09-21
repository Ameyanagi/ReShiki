"""Independent 3D atom stereo assignments through RDKit's public API."""

import itertools
import json
import math
import random
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase
from rdkit.Chem import rdDepictor

if TYPE_CHECKING or __package__:
    from .kekulize_reference import directions
    from .ranking_reference import metadata
    from .valence_reference import ORDERS, graph
else:
    from kekulize_reference import directions
    from ranking_reference import metadata
    from valence_reference import ORDERS, graph


def annotations(mol):
    return dict(
        non_explicit=[
            a.GetIntProp("_NonExplicit3DChirality")
            if a.HasProp("_NonExplicit3DChirality")
            else None
            for a in mol.GetAtoms()
        ],
        done=bool(mol.GetPropsAsDict(True, True)["_StereochemDone"])
        if mol.HasProp("_StereochemDone")
        else None,
    )


def emit(name, original, replace=True, non_tetra=True):
    mol = Chem.Mol(original)
    mol.UpdatePropertyCache(strict=False)
    before = dict(
        graph=graph(mol),
        metadata=metadata(mol),
        directions=directions(mol),
        annotations=annotations(mol),
    )
    conf = None
    if mol.GetNumConformers():
        conformer = mol.GetConformer()
        conf = dict(
            is_3d=conformer.Is3D(),
            positions=[
                dict(x=p.x, y=p.y, z=p.z)
                for p in (conformer.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
            ],
        )
    previous = Chem.GetAllowNontetrahedralChirality()
    try:
        Chem.SetAllowNontetrahedralChirality(non_tetra)
        Chem.AssignAtomChiralTagsFromStructure(mol, replaceExistingTags=replace)
        expected = dict(metadata=metadata(mol), annotations=annotations(mol))
        if graph(mol) != before["graph"] or directions(mol) != before["directions"]:
            raise AssertionError(
                "3D atom perception unexpectedly edited chemistry or bond directions"
            )
        failure = None
    except (RuntimeError, ValueError) as error:
        expected, failure = None, str(error)
    finally:
        Chem.SetAllowNontetrahedralChirality(previous)
    print(
        json.dumps(
            dict(
                name=f"{name}/{replace}/{non_tetra}",
                before=before,
                conformer=conf,
                options=dict(replace_existing=replace, allow_nontetrahedral=non_tetra),
                expected=expected,
                failure=failure,
            )
        )
    )


def star(number, points, order=1, reverse=False, hydrogens=0, no_implicit=False):
    mol = Chem.RWMol()
    atom = Chem.Atom(number)
    atom.SetNumExplicitHs(hydrogens)
    atom.SetNoImplicit(no_implicit)
    mol.AddAtom(atom)
    conf = Chem.Conformer(len(points) + 1)
    conf.Set3D(True)
    for i, point in enumerate(points, 1):
        mol.AddAtom(Chem.Atom((9, 17, 35, 53, 8, 7, 6, 6)[(i - 1) % 8]))
        a, b = (i, 0) if reverse and i == 1 else (0, i)
        mol.AddBond(a, b, ORDERS[order] if i == 1 else Chem.BondType.SINGLE)
        conf.SetAtomPosition(i, point)
    mol.AddConformer(conf)
    return mol


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    rng = random.Random(455205)
    octahedron = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
    tetrahedron = [(1, 1, 1), (1, -1, -1), (-1, 1, -1), (-1, -1, 1)]
    planar = [(1, 0, 0), (0, 1, 0), (-1, 0, 0), (0, -1, 0)]
    bipyramid = [
        (0, 0, 1),
        (0, 0, -1),
        (1, 0, 0),
        (-0.5, math.sqrt(3) / 2, 0),
        (-0.5, -math.sqrt(3) / 2, 0),
    ]
    layouts = [
        tetrahedron,
        tetrahedron[:3],
        planar,
        planar[:3],
        bipyramid,
        bipyramid[:4],
        octahedron[:5],
        octahedron,
    ]
    # All coordination permutations, reflected hands, missing ligands and the
    # reference's choice of the first three nonzero-bond neighbors.
    for layout, points in enumerate(layouts):
        for permutation, shuffled in enumerate(itertools.permutations(points)):
            for number in (6, 16, 78):
                for mirror in (-1, 1):
                    reflected = [(x * mirror, y, z) for x, y, z in shuffled]
                    mol = star(number, reflected)
                    emit(f"layout/{layout}/{permutation}/{number}/{mirror}", mol)
    for number, degree, order in itertools.product(range(119), range(1, 8), ORDERS):
        points = [(math.cos(i), math.sin(i), (i % 3 - 1) * 0.6) for i in range(degree)]
        mol = star(number, points, order, reverse=bool(degree % 2))
        emit(f"element/{number}/{degree}/{order}", mol)
    for sample in range(3000):
        size = rng.randrange(3, 8)
        mol = star(
            rng.choice((0, 6, 7, 14, 15, 16, 33, 34, 78)),
            [(rng.uniform(-2, 2), rng.uniform(-2, 2), rng.uniform(-2, 2)) for _ in range(size)],
            order=rng.choice(list(ORDERS)),
            reverse=bool(sample % 2),
            hydrogens=rng.randrange(3),
            no_implicit=bool(sample % 3),
        )
        atom = mol.GetAtomWithIdx(0)
        atom.SetChiralTag(Chem.ChiralType.values[sample % 9])
        if sample % 4 == 0:
            atom.SetUnsignedProp("_chiralPermutation", sample % 31)
        if sample % 3 == 0:
            atom.SetIntProp("_NonExplicit3DChirality", sample % 4 - 1)
        if sample % 2:
            mol.SetBoolProp("_StereochemDone", sample % 3 == 0)
        bond = mol.GetBondWithIdx(rng.randrange(size))
        bond.SetBondDir(Chem.BondDir.values[sample % 7])
        if sample % 5 == 0:
            bond.SetIntProp("_UnknownStereo", 1)
        for replace in (False, True):
            emit(f"random/{sample}", mol, replace, non_tetra=bool(sample % 2))
    # Geometric tolerance boundaries and zero-length normalization errors.
    for number, z in itertools.product(
        (6, 15, 16, 34, 78), (-0.1000001, -0.1, -0.0999999, 0, 0.0999999, 0.1, 0.1000001)
    ):
        for fourth in (False, True):
            points = [(1, 0, 0), (0, 1, 0), (0.2, 0.1, z)] + ([(0, 0, 1)] if fourth else [])
            emit(f"volume/{number}/{z}/{fourth}", star(number, points))
    for number, degree, zero in itertools.product(
        (6, 15, 78), range(3, 8), (0.0, 1e-17, 1e-16, 1e-15)
    ):
        points = (octahedron + [(1, 1, 1)])[:degree]
        points[0] = (zero, 0, 0)
        emit(f"normalization/{number}/{degree}/{zero}", star(number, points))
    for conformer in ("none", "2d", "3d"):
        mol = star(6, tetrahedron)
        if conformer == "none":
            mol.RemoveAllConformers()
        else:
            mol.GetConformer().Set3D(conformer == "3d")
        mol.SetBoolProp("_StereochemDone", False)
        mol.GetAtomWithIdx(0).SetChiralTag(Chem.ChiralType.CHI_TETRAHEDRAL_CW)
        for replace in (False, True):
            emit(f"conformer/{conformer}", mol, replace)
    # Real 3D molecules retain their source coordinates. 2D data gets a bounded
    # Z perturbation to exercise different local neighborhoods deterministically.
    for i, mol in enumerate(
        Chem.SDMolSupplier(
            str(Path(RDConfig.RDDataDir) / "NCI/first_200.props.sdf"), removeHs=False
        )
    ):
        if mol is None:
            continue
        if not mol.GetNumConformers():
            rdDepictor.Compute2DCoords(mol)
        conf = mol.GetConformer()
        conf.Set3D(True)
        for a in mol.GetAtoms():
            pos = conf.GetAtomPosition(a.GetIdx())
            conf.SetAtomPosition(a.GetIdx(), (pos.x, pos.y, rng.uniform(-0.7, 0.7)))
        emit(f"NCI/{i}", mol)


if __name__ == "__main__":
    main()
