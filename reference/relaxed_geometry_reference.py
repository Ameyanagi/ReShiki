"""Pinned RDKit UFF oracle for relaxed metal geometry (issue #371).

Regenerate the committed fixture from the repository root:
  uv run --locked python reference/relaxed_geometry_reference.py \
      --output tests/fixtures/relaxed-geometry-rdkit.json

This compares the unchanged dative/covalent graph, with original atoms first
and AddHs-appended hydrogen coordinates after them. RDKit's generic sanitizer
assigns cisplatin Pt SP3D2, for which UFF has no Pt6+2 parameters. The explicit
SP2D override below selects the tabulated square-planar Pt4+2 model. It is a
coordination-model choice, not an experimental structure prediction.
"""

import argparse
import json
import math
from itertools import combinations
from pathlib import Path

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import AllChem

VERSION = "2026.03.6"
SEED = 61453
MAX_ITERATIONS = 2000


def complex_molecule(metal_number, charge, donors, chlorides=0):
    builder = Chem.RWMol()
    metal = Chem.Atom(metal_number)
    metal.SetFormalCharge(charge)
    metal.SetNoImplicit(True)
    builder.AddAtom(metal)
    for _ in range(donors):
        nitrogen = Chem.Atom(7)
        nitrogen.SetNumExplicitHs(3)
        nitrogen.SetNoImplicit(True)
        index = builder.AddAtom(nitrogen)
        builder.AddBond(index, 0, Chem.BondType.DATIVE)
    for _ in range(chlorides):
        chlorine = Chem.Atom(17)
        chlorine.SetNoImplicit(True)
        index = builder.AddAtom(chlorine)
        builder.AddBond(0, index, Chem.BondType.SINGLE)
    molecule = builder.GetMol()
    molecule.UpdatePropertyCache(strict=False)
    excluded = (
        Chem.SanitizeFlags.SANITIZE_PROPERTIES
        | Chem.SanitizeFlags.SANITIZE_FINDRADICALS
        | Chem.SanitizeFlags.SANITIZE_ADJUSTHS
        | Chem.SanitizeFlags.SANITIZE_CLEANUP_ORGANOMETALLICS
    )
    Chem.SanitizeMol(molecule, sanitizeOps=Chem.SanitizeFlags.SANITIZE_ALL ^ excluded)
    automatic_hybridization = str(molecule.GetAtomWithIdx(0).GetHybridization())
    automatic_has_parameters = AllChem.UFFHasAllMoleculeParams(Chem.AddHs(molecule))
    if metal_number == 78:
        molecule.GetAtomWithIdx(0).SetHybridization(Chem.HybridizationType.SP2D)
    if not AllChem.UFFHasAllMoleculeParams(Chem.AddHs(molecule)):
        raise ValueError("Selected coordination model lacks RDKit UFF parameters")
    return molecule, automatic_hybridization, automatic_has_parameters


def unit(vector):
    norm = math.sqrt(sum(value * value for value in vector))
    return tuple(value / norm for value in vector)


def cross(first, second):
    x, y, z = first
    a, b, c = second
    return (y * c - z * b, z * a - x * c, x * b - y * a)


def with_coordinates(molecule, points):
    """Assign every H deterministically, independent of embedding machinery."""
    molecule = Chem.Mol(molecule)
    conformer = Chem.Conformer(molecule.GetNumAtoms())
    conformer.Set3D(True)
    for index, point in enumerate(points):
        conformer.SetAtomPosition(index, point)
    molecule.RemoveAllConformers()
    molecule.AddConformer(conformer, assignId=True)
    original_count = molecule.GetNumAtoms()
    calculation = Chem.AddHs(molecule, addCoords=True)
    conformer = calculation.GetConformer()
    parents = []
    for parent in range(original_count):
        hydrogens = [
            atom.GetIdx()
            for atom in calculation.GetAtomWithIdx(parent).GetNeighbors()
            if atom.GetIdx() >= original_count
        ]
        if not hydrogens:
            continue
        if len(hydrogens) != 3 or calculation.GetAtomWithIdx(parent).GetAtomicNum() != 7:
            raise ValueError("Reference expects three added H on each ammonia donor")
        outward = unit(tuple(points[parent][axis] - points[0][axis] for axis in range(3)))
        axis = (0.0, 0.0, 1.0) if abs(outward[2]) < 0.8 else (0.0, 1.0, 0.0)
        first = unit(cross(outward, axis))
        second = cross(outward, first)
        for ordinal, hydrogen in enumerate(hydrogens):
            phase = 2.0 * math.pi * ordinal / 3.0 + parent * 0.2
            offset = tuple(
                outward[axis] / 3.0
                + math.sqrt(8.0 / 9.0)
                * (math.cos(phase) * first[axis] + math.sin(phase) * second[axis])
                for axis in range(3)
            )
            conformer.SetAtomPosition(
                hydrogen, tuple(points[parent][axis] + 1.02 * offset[axis] for axis in range(3))
            )
            parents.append(parent)
    return calculation, parents


def coordinates(molecule):
    return [list(point) for point in molecule.GetConformer().GetPositions()]


def force_field(molecule):
    # Every oracle graph has one connected component. Enabling interfragment
    # interactions leaves its pair set unchanged and avoids RDKit's hidden
    # strict sanitization of fragment copies in the default fragment path.
    result = AllChem.UFFGetMoleculeForceField(
        molecule, vdwThresh=float("inf"), confId=0, ignoreInterfragInteractions=False
    )
    if result is None:
        raise ValueError("RDKit failed to construct the UFF force field")
    result.Initialize()
    return result


def coordination(points, ligand_count):
    vectors = [tuple(point[i] - points[0][i] for i in range(3)) for point in points[1:]]
    vectors = vectors[:ligand_count]
    directions = [unit(vector) for vector in vectors]
    return dict(
        bond_lengths=[math.sqrt(sum(value * value for value in vector)) for vector in vectors],
        angles=[
            dict(
                a=first + 1,
                b=second + 1,
                degrees=math.degrees(
                    math.acos(
                        max(
                            -1.0,
                            min(
                                1.0,
                                sum(a * b for a, b in zip(directions[first], directions[second])),
                            ),
                        )
                    )
                ),
            )
            for first, second in combinations(range(ligand_count), 2)
        ],
    )


def minimize(molecule):
    calculation = Chem.Mol(molecule)
    field = force_field(calculation)
    initial_energy = field.CalcEnergy()
    status = field.Minimize(maxIts=MAX_ITERATIONS, forceTol=1e-4, energyTol=1e-6)
    return dict(
        initial_energy=initial_energy,
        energy=field.CalcEnergy(),
        status=status,
        coordinates=coordinates(calculation),
    )


def initialization_comparison(molecule, planar_points):
    """Capture failures as well as energies: ETKDG need not support a metal."""
    planar, _ = with_coordinates(molecule, planar_points)
    drawn = minimize(planar)
    del drawn["coordinates"]
    result = {"drawn_perturbed_planar": drawn}
    for random_coordinates in (False, True):
        calculation = Chem.AddHs(Chem.Mol(molecule))
        parameters = AllChem.ETKDGv3()
        parameters.randomSeed = SEED
        parameters.numThreads = 1
        parameters.maxIterations = 200
        parameters.useRandomCoords = random_coordinates
        parameters.trackFailures = True
        identifier = AllChem.EmbedMolecule(calculation, parameters)
        entry = dict(conformer_id=identifier, failures=list(parameters.GetFailureCounts()))
        if identifier >= 0:
            minimized = minimize(calculation)
            del minimized["coordinates"]
            entry.update(minimized)
        result["etkdg_v3_random" if random_coordinates else "etkdg_v3"] = entry
    return result


def request(molecule, points):
    return dict(
        atoms=[
            dict(
                atomic_number=atom.GetAtomicNum(),
                isotope=atom.GetIsotope(),
                charge=atom.GetFormalCharge(),
                explicit_h=atom.GetNumExplicitHs(),
                no_implicit=atom.GetNoImplicit(),
                aromatic=atom.GetIsAromatic(),
                radical=atom.GetNumRadicalElectrons(),
                chiral_tag=int(atom.GetChiralTag()),
            )
            for atom in molecule.GetAtoms()
        ],
        bonds=[
            dict(
                a=bond.GetBeginAtomIdx(),
                b=bond.GetEndAtomIdx(),
                order=int(bond.GetBondType()),
                aromatic=bond.GetIsAromatic(),
                stereo=int(bond.GetStereo()),
                stereo_atoms=list(bond.GetStereoAtoms()) or None,
            )
            for bond in molecule.GetBonds()
        ],
        field="UFF",
        operation="Evaluate",
        relaxed=True,
        coordinates=points,
        fixed_atoms=[],
        conformers=1,
        seed=SEED,
        max_iterations=MAX_ITERATIONS,
    )


def case(name, metal_number, charge, donors, chlorides, points, planar_points, uff_type):
    molecule, automatic_hybridization, automatic_has_parameters = complex_molecule(
        metal_number, charge, donors, chlorides
    )
    calculation, parents = with_coordinates(molecule, points)
    minimized = minimize(calculation)
    initial = coordinates(calculation)
    return dict(
        name=name,
        metal_uff_type=uff_type,
        automatic_metal_hybridization=automatic_hybridization,
        automatic_has_parameters=automatic_has_parameters,
        selected_metal_hybridization=str(molecule.GetAtomWithIdx(0).GetHybridization()),
        original_count=molecule.GetNumAtoms(),
        hydrogen_parents=parents,
        request=request(molecule, initial),
        energy=minimized["initial_energy"],
        optimized_energy=minimized["energy"],
        optimization_status=minimized["status"],
        optimized_coordinates=minimized["coordinates"],
        optimized_coordination=coordination(minimized["coordinates"], donors + chlorides),
        initialization=initialization_comparison(molecule, planar_points),
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="Write fixture JSON; defaults to stdout")
    arguments = parser.parse_args()
    if rdBase.rdkitVersion != VERSION:
        raise RuntimeError(f"Expected RDKit {VERSION}; found {rdBase.rdkitVersion}")
    RDLogger.DisableLog("rdApp.*")
    platinum_points = [
        (0.0, 0.0, 0.0),
        (2.0, 0.0, 0.15),
        (0.0, 2.1, -0.1),
        (-2.25, 0.0, 0.0),
        (0.0, -2.4, 0.1),
    ]
    cobalt_points = [
        (0.0, 0.0, 0.0),
        (1.95, 0.0, 0.05),
        (-2.0, 0.05, 0.0),
        (0.0, 1.9, -0.05),
        (0.05, -1.95, 0.0),
        (0.0, 0.05, 2.0),
        (-0.05, 0.0, -1.9),
    ]
    cobalt_planar = [(0.0, 0.0, 0.0)] + [
        (
            2.0 * math.cos(index * math.pi / 3.0),
            2.0 * math.sin(index * math.pi / 3.0),
            0.03 * (index - 2),
        )
        for index in range(6)
    ]
    result = dict(
        rdkit_version=VERSION,
        cases=[
            case("cisplatin", 78, 2, 2, 2, platinum_points, platinum_points, "Pt4+2"),
            case("hexaammine_cobalt_iii", 27, 3, 6, 0, cobalt_points, cobalt_planar, "Co6+3"),
        ],
    )
    serialized = json.dumps(result, indent=2, allow_nan=False) + "\n"
    if arguments.output:
        arguments.output.write_text(serialized)
    else:
        print(serialized, end="")


if __name__ == "__main__":
    main()
