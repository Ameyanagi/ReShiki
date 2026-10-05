"""Optional independent RDKit oracle for the native application geometry worker.

Run after building the app, using the locked development RDKit environment:
  RESHIKI_GEOMETRY_REFERENCE_APP=/absolute/path/to/reshiki \
  RESHIKI_REQUIRE_GEOMETRY_REFERENCE=1 \
  uv run --locked python -m unittest tests.test_geometry_reference

The application never executes Python. Python is only this development oracle.
Sampling is not compared across compilers: force-field values are compared at
identical Cartesian coordinates in angstroms. Energy tolerance is
1e-6 + 1e-8*abs(E) kcal/mol; each gradient component tolerance is
1e-5 + 1e-7*abs(g) kcal/(mol angstrom). These checks establish local optimization
and reference parity, not a global minimum or a validated chemical model.
"""

import copy
import json
import math
import os
import struct
import subprocess
import unittest
from pathlib import Path

try:
    from rdkit import Chem, rdBase
    from rdkit.Chem import AllChem
except ImportError:
    Chem = AllChem = rdBase = None

VERSION = "2026.03.6"
FIELDS = ("MMFF94", "MMFF94s", "UFF")
FIXTURES = {
    "ethanol": "CCO",
    "butane": "CCCC",
    "acetamide": "CC(=O)N",
    "benzene": "c1ccccc1",
    "cyclohexane": "C1CCCCC1",
    "fused_aromatic": "c1ccc2ccccc2c1",
    "fused_saturated": "C1CCC2CCCCC2C1",
    "tetrahedral_implicit_h": "C[C@H](O)F",
    "tetrahedral_no_h": "N[C@@](C)(O)C(=O)O",
    "tetrahedral_original_isotope_h": "[2H][C@](F)(Cl)Br",
    "alkene_e": "C/C=C/C",
    "alkene_z": "C/C=C\\C",
    "ammonium": "C[NH3+]",
    "carboxylate": "CC(=O)[O-]",
    "zwitterion": "[NH3+]CC(=O)[O-]",
}
APPLICATION = os.environ.get("RESHIKI_GEOMETRY_REFERENCE_APP") or os.environ.get(
    "RESHIKI_TEST_PACKAGED_APP"
)
REQUIRED = os.environ.get("RESHIKI_REQUIRE_GEOMETRY_REFERENCE") == "1"


def molecule_request(smiles, field="MMFF94"):
    """Keep RDKit atom and bond insertion ordering; do not canonicalize indices."""
    molecule = Chem.MolFromSmiles(smiles)
    if molecule is None:
        raise ValueError(f"Invalid reference SMILES: {smiles}")
    atoms = [
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
    ]
    bonds = [
        dict(
            a=bond.GetBeginAtomIdx(),
            b=bond.GetEndAtomIdx(),
            order=int(bond.GetBondType()),
            aromatic=bond.GetIsAromatic(),
            stereo=int(bond.GetStereo()),
            stereo_atoms=list(bond.GetStereoAtoms()) or None,
        )
        for bond in molecule.GetBonds()
    ]
    request = dict(
        atoms=atoms,
        bonds=bonds,
        field=field,
        operation="Generate",
        coordinates=[],
        fixed_atoms=[],
        conformers=2,
        seed=61453,
        max_iterations=2000,
    )
    return molecule, request


def native_fixture_request(name):
    """Reconstruct the exact native graph with RDKit, retaining original H atoms."""
    directory = Path(__file__).resolve().parents[1] / "native" / "geometry" / "tests" / "fixtures"
    request = json.loads((directory / name).read_text())
    builder = Chem.RWMol()
    for record in request["atoms"]:
        atom = Chem.Atom(record["atomic_number"])
        atom.SetIsotope(record["isotope"])
        atom.SetFormalCharge(record["charge"])
        atom.SetNumExplicitHs(record["explicit_h"])
        atom.SetNoImplicit(record["no_implicit"])
        atom.SetIsAromatic(record["aromatic"])
        atom.SetNumRadicalElectrons(record["radical"])
        atom.SetChiralTag(Chem.ChiralType.values[record["chiral_tag"]])
        builder.AddAtom(atom)
    for record in request["bonds"]:
        builder.AddBond(record["a"], record["b"], Chem.BondType.values[record["order"]])
        bond = builder.GetBondBetweenAtoms(record["a"], record["b"])
        bond.SetIsAromatic(record["aromatic"])
        if record["stereo_atoms"]:
            bond.SetStereoAtoms(*record["stereo_atoms"])
        bond.SetStereo(Chem.BondStereo.values[record["stereo"]])
    molecule = builder.GetMol()
    Chem.SanitizeMol(molecule)
    return molecule, request


def reference_field(molecule, coordinates, field):
    """Construct separately through RDKit's public Python chemistry APIs."""
    calculation = Chem.AddHs(Chem.Mol(molecule))
    if calculation.GetNumAtoms() != len(coordinates):
        raise ValueError("Reference and native added-hydrogen counts differ")
    conformer = Chem.Conformer(len(coordinates))
    for index, point in enumerate(coordinates):
        conformer.SetAtomPosition(index, point)
    conformer.Set3D(True)
    calculation.RemoveAllConformers()
    calculation.AddConformer(conformer, assignId=True)
    if field == "UFF":
        if not AllChem.UFFHasAllMoleculeParams(calculation):
            raise ValueError("Reference UFF parameters unavailable")
        force_field = AllChem.UFFGetMoleculeForceField(
            calculation, vdwThresh=float("inf"), confId=0, ignoreInterfragInteractions=True
        )
    else:
        properties = AllChem.MMFFGetMoleculeProperties(calculation, mmffVariant=field)
        if properties is None:
            raise ValueError("Reference MMFF parameters unavailable")
        force_field = AllChem.MMFFGetMoleculeForceField(
            calculation,
            properties,
            nonBondedThresh=float("inf"),
            confId=0,
            ignoreInterfragInteractions=True,
        )
    if force_field is None:
        raise ValueError("Reference force field unavailable")
    force_field.Initialize()
    return calculation, force_field


def rotate_translate(point):
    # Proper orthogonal rotation; unit conversion/scale is deliberately absent.
    x, y, z = point
    return [-y + 17.0, x - 11.0, z + 4.0]


class GeometryReferenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        missing = None
        if Chem is None:
            missing = "Optional RDKit development oracle is not installed"
        elif rdBase.rdkitVersion != VERSION:
            missing = f"Expected RDKit {VERSION}; found {rdBase.rdkitVersion}"
        elif not APPLICATION or not Path(APPLICATION).is_file():
            missing = "Set RESHIKI_GEOMETRY_REFERENCE_APP to the built application"
        if missing:
            if REQUIRED:
                raise RuntimeError(missing)
            raise unittest.SkipTest(missing)
        cls.application = Path(APPLICATION).resolve()

    def worker_result(self, operation):
        body = json.dumps(
            dict(heap_bytes=64 * 1024 * 1024, operation=operation),
            separators=(",", ":"),
            allow_nan=False,
        ).encode()
        frame = b"RSHGEOM1" + struct.pack("<HHI", 1, 0, len(body)) + body
        completed = subprocess.run(
            [str(self.application), "--geometry-worker"],
            input=frame,
            capture_output=True,
            timeout=45,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr.decode(errors="replace"))
        answer = completed.stdout
        self.assertLessEqual(len(answer), 4 * 1024 * 1024)
        self.assertEqual(answer[:12], b"RSHGEOM1\x01\x00\x00\x00")
        self.assertEqual(struct.unpack("<I", answer[12:16])[0], len(answer) - 16)
        response = json.loads(answer[16:])
        self.assertEqual(response["version"], VERSION)
        return response["result"]

    def solve(self, request):
        result = self.worker_result(request)
        self.assertIn("Ok", result, result.get("Err", "Missing geometry response"))
        response = result["Ok"]
        self.assertEqual(response["field"], request["field"])
        self.assertEqual(response["original_count"], len(request["atoms"]))
        self.assertTrue(all(math.isfinite(x) for p in response["coordinates"] for x in p))
        self.assertTrue(math.isfinite(response["energy"]))
        return response

    def assert_energy(self, actual, expected):
        self.assertAlmostEqual(actual, expected, delta=1e-6 + 1e-8 * abs(expected))

    def assert_gradient(self, actual, expected):
        flattened = [component for point in actual for component in point]
        self.assertEqual(len(flattened), len(expected))
        for index, (got, want) in enumerate(zip(flattened, expected, strict=True)):
            self.assertTrue(math.isfinite(got), index)
            self.assertAlmostEqual(
                got, want, delta=1e-5 + 1e-7 * abs(want), msg=f"gradient {index}"
            )

    def assert_parity(self, molecule, request, coordinates):
        evaluation = copy.deepcopy(request)
        evaluation.update(operation="Evaluate", coordinates=coordinates, fixed_atoms=[])
        result = self.solve(evaluation)
        _, reference = reference_field(molecule, coordinates, request["field"])
        self.assert_energy(result["energy"], reference.CalcEnergy())
        self.assert_gradient(result["gradient"], reference.CalcGrad())
        return result

    def test_public_fixtures_match_official_same_coordinate_energy_and_gradient(self):
        for name, smiles in FIXTURES.items():
            for field in FIELDS:
                with self.subTest(fixture=name, field=field):
                    molecule, request = molecule_request(smiles, field)
                    generated = self.solve(request)
                    self.assertTrue(generated["converged"])
                    self.assertLessEqual(generated["energy"], generated["initial_energy"] + 1e-6)
                    self.assertIsNone(generated["gradient"])
                    calculation = Chem.AddHs(Chem.Mol(molecule))
                    n = molecule.GetNumAtoms()
                    parents = [
                        calculation.GetAtomWithIdx(i).GetNeighbors()[0].GetIdx()
                        for i in range(n, calculation.GetNumAtoms())
                    ]
                    self.assertEqual(generated["hydrogen_parents"], parents)
                    evaluated = self.assert_parity(molecule, request, generated["coordinates"])
                    self.assert_energy(generated["energy"], evaluated["energy"])

    def test_rigid_transform_rotates_gradient_without_changing_energy(self):
        for field in FIELDS:
            with self.subTest(field=field):
                molecule, request = molecule_request("CC(=O)N", field)
                coordinates = self.solve(request)["coordinates"]
                # Move away from the local minimum to test meaningful forces.
                coordinates[0][0] += 0.07
                original = self.assert_parity(molecule, request, coordinates)
                moved = self.assert_parity(
                    molecule, request, list(map(rotate_translate, coordinates))
                )
                self.assert_energy(moved["energy"], original["energy"])
                rotated_gradient = [
                    value for x, y, z in original["gradient"] for value in (-y, x, z)
                ]
                self.assert_gradient(moved["gradient"], rotated_gradient)

    def test_complete_c60_generation_and_existing_xyz_match_independent_force_fields(self):
        fixture_directory = (
            Path(__file__).resolve().parents[1] / "native" / "geometry" / "tests" / "fixtures"
        )
        request = json.loads((fixture_directory / "c60-request.json").read_text())
        reference = json.loads((fixture_directory / "c60-reference.json").read_text())
        self.assertEqual(reference["rdkit_version"], VERSION)
        molecule = Chem.MolFromInchi(reference["source_inchi"])
        self.assertIsNotNone(molecule)
        self.assertEqual(molecule.GetNumAtoms(), 60)
        self.assertEqual(molecule.GetNumBonds(), 90)
        # The reference is independently reconstructed from the user's InChI,
        # keeping the captured graph's original insertion order and indices.
        for captured, bond in zip(request["bonds"], molecule.GetBonds(), strict=True):
            self.assertEqual(captured["a"], bond.GetBeginAtomIdx())
            self.assertEqual(captured["b"], bond.GetEndAtomIdx())
            self.assertEqual(captured["order"], int(bond.GetBondType()))
        for field in FIELDS:
            with self.subTest(field=field):
                request["field"] = field
                request["coordinates"] = []
                generated = self.solve(request)
                self.assertTrue(generated["converged"])
                self.assertEqual(len(generated["coordinates"]), 60)
                self.assertEqual(generated["hydrogen_parents"], [])
                evaluated = self.assert_parity(molecule, request, generated["coordinates"])
                self.assert_energy(generated["energy"], evaluated["energy"])
                request["coordinates"] = reference["coordinates"]
                _, initial = reference_field(molecule, reference["coordinates"], field)
                reused = self.solve(request)
                self.assertTrue(reused["converged"])
                self.assertTrue(
                    any(
                        "reused existing original-atom 3D coordinates" in line
                        for line in reused["diagnostics"]
                    ),
                    reused["diagnostics"],
                )
                self.assert_energy(reused["initial_energy"], initial.CalcEnergy())
                self.assert_parity(molecule, request, reused["coordinates"])

    def test_native_import_embedding_regressions_match_official_force_fields(self):
        cases = (
            ("taxol70-request.json", ("MMFF94s",), 70, 43, (None,)),
            ("user-c36-request.json", FIELDS, 36, 24, (1, None)),
        )
        for fixture, fields, original_count, added_h, samples in cases:
            for field, conformers in ((field, count) for field in fields for count in samples):
                with self.subTest(
                    fixture=fixture, field=field, conformers=conformers or "fixture default"
                ):
                    molecule, request = native_fixture_request(fixture)
                    self.assertEqual(molecule.GetNumAtoms(), original_count)
                    self.assertEqual(request["conformers"], 8)
                    request.update(field=field)
                    if conformers is not None:
                        request["conformers"] = conformers
                    source = copy.deepcopy(request)
                    generated = self.solve(request)
                    self.assertEqual(len(generated["coordinates"]), original_count + added_h)
                    self.assertLessEqual(generated["energy"], generated["initial_energy"] + 1e-6)
                    self.assertEqual(request, source)
                    calculation, _ = reference_field(molecule, generated["coordinates"], field)
                    parents = [
                        calculation.GetAtomWithIdx(i).GetNeighbors()[0].GetIdx()
                        for i in range(original_count, calculation.GetNumAtoms())
                    ]
                    self.assertEqual(generated["hydrogen_parents"], parents)
                    self.assertEqual(
                        [
                            i
                            for i in range(original_count)
                            if calculation.GetAtomWithIdx(i).GetAtomicNum() == 1
                        ],
                        list(range(62, 70)) if original_count == 70 else [],
                    )
                    # Independent 3D stereo assignment must recover every
                    # original specified configuration in the same index order.
                    Chem.AssignAtomChiralTagsFromStructure(
                        calculation, confId=0, replaceExistingTags=True
                    )
                    for index, record in enumerate(request["atoms"]):
                        if record["chiral_tag"]:
                            self.assertEqual(
                                int(calculation.GetAtomWithIdx(index).GetChiralTag()),
                                record["chiral_tag"],
                                index,
                            )
                    if original_count == 36:
                        self.assertTrue(generated["converged"])
                        self.assertFalse(
                            any("cage-ETDG" in line for line in generated["diagnostics"])
                        )
                    evaluated = self.assert_parity(molecule, request, generated["coordinates"])
                    self.assert_energy(generated["energy"], evaluated["energy"])

    def test_fixed_original_and_temporary_hydrogen_stay_exact_in_relaxation(self):
        for field in FIELDS:
            with self.subTest(field=field):
                molecule, request = molecule_request("CCO", field)
                coordinates = self.solve(request)["coordinates"]
                coordinates[1][0] += 0.08
                request.update(operation="Relax", coordinates=coordinates, fixed_atoms=[0, 3])
                relaxed = self.solve(request)
                self.assertEqual(relaxed["coordinates"][0], coordinates[0])
                self.assertEqual(relaxed["coordinates"][3], coordinates[3])
                _, initial = reference_field(molecule, coordinates, field)
                self.assert_energy(relaxed["initial_energy"], initial.CalcEnergy())
                self.assertLessEqual(relaxed["energy"], relaxed["initial_energy"] + 1e-6)
                self.assert_parity(molecule, request, relaxed["coordinates"])
                # Independently minimize from the same starting geometry with
                # the same physical field and fixed coordinates. Compare final
                # energy, allowing optimizer/compiler termination differences.
                calculation, reference = reference_field(molecule, coordinates, field)
                reference.AddFixedPoint(0)
                reference.AddFixedPoint(3)
                reference.Minimize(maxIts=2000, forceTol=1e-4, energyTol=1e-6)
                self.assertAlmostEqual(relaxed["energy"], reference.CalcEnergy(), delta=1e-4)
                final = calculation.GetConformer()
                for index in (0, 3):
                    self.assertEqual(list(final.GetAtomPosition(index)), coordinates[index])

    def test_charged_boron_uff_has_no_implicit_mmff_fallback(self):
        for smiles in ("B(O)O", "[B-](F)(F)(F)F"):
            molecule, request = molecule_request(smiles, "UFF")
            calculation = Chem.AddHs(Chem.Mol(molecule))
            self.assertTrue(AllChem.UFFHasAllMoleculeParams(calculation))
            self.assertFalse(AllChem.MMFFHasAllMoleculeParams(calculation))
            generated = self.solve(request)
            self.assert_parity(molecule, request, generated["coordinates"])
            for field in ("MMFF94", "MMFF94s"):
                request["field"] = field
                result = self.worker_result(request)
                self.assertIn("Err", result)
                self.assertIn("MMFF parameters", result["Err"])
        for field in FIELDS:
            _, request = molecule_request("[Zn+2](Cl)Cl", field)
            result = self.worker_result(request)
            self.assertIn("Err", result)
            self.assertTrue(result["Err"])
            _, disconnected = molecule_request("CCO.CN", field)
            rejected = self.worker_result(disconnected)
            self.assertIn("Err", rejected)
            self.assertIn("one connected covalent component", rejected["Err"])

    def test_drawing_scale_is_not_a_force_field_unit_conversion(self):
        molecule, request = molecule_request("CCO", "MMFF94")
        coordinates = self.solve(request)["coordinates"]
        original = self.assert_parity(molecule, request, coordinates)
        doubled = self.assert_parity(
            molecule, request, [[2 * value for value in p] for p in coordinates]
        )
        self.assertGreater(doubled["energy"], original["energy"] + 1.0)


if __name__ == "__main__":
    unittest.main()
