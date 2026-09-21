"""The migrated worker supplies a sanitized graph without derived H or descriptors."""

import copy
import unittest
from unittest.mock import patch

from rdkit import Chem, rdBase

from engine.worker import analyze, handle


def imported(smiles):
    return handle(dict(protocol=1, operation="import", format="smiles", text=smiles))


class LocalPropertyTests(unittest.TestCase):
    def test_every_analysis_path_skips_migrated_descriptors(self):
        doc = imported("COc1ccccc1")["document"]
        reaction = "[CH3:1][OH:2]>>[CH2:1]=[O:2]"
        requests = [
            dict(operation="import", format="smiles", text="[13CH3][NH3+]"),
            dict(operation="import", format="rsmi", text=reaction),
            dict(operation="analyze", document=doc),
            dict(operation="export", document=doc, format="smiles"),
            dict(operation="clean", document=doc),
            dict(operation="aromatic", document=doc, selected_ids=[a["id"] for a in doc["atoms"]]),
            dict(operation="abbreviate", document=doc, text="OMe"),
        ]
        for request in requests:
            with (
                self.subTest(operation=request["operation"], format=request.get("format")),
                patch("engine.worker.rdMolDescriptors.CalcMolFormula", side_effect=AssertionError),
                patch("engine.worker.rdMolDescriptors._CalcMolWt", side_effect=AssertionError),
                patch("engine.worker.rdMolDescriptors.CalcExactMolWt", side_effect=AssertionError),
                patch(
                    "engine.worker.rdMolDescriptors.CalcCrippenDescriptors",
                    side_effect=AssertionError,
                ),
                patch("engine.worker.rdMolDescriptors.CalcTPSA", side_effect=AssertionError),
                patch("engine.worker.rdMolDescriptors.CalcNumHBD", side_effect=AssertionError),
                patch("engine.worker.rdMolDescriptors.CalcNumHBA", side_effect=AssertionError),
                patch("engine.worker.rdMolDescriptors.CalcNumRings", side_effect=AssertionError),
            ):
                result = handle(dict(protocol=1, local_properties=True, **copy.deepcopy(request)))
                analysis = result["analysis"]
                for key in (
                    "formula",
                    "mass",
                    "exact_mass",
                    "unpaired_electrons",
                    "rings",
                    "logp",
                    "tpsa",
                    "donors",
                    "acceptors",
                ):
                    self.assertNotIn(key, analysis)
                facts = analysis["property_input"]
                self.assertEqual(facts["rdkit_version"], rdBase.rdkitVersion)
                self.assertNotIn("reference_rings", facts)
                self.assertEqual(len(facts["graph"]["atoms"]), len(result["document"]["atoms"]))
                self.assertTrue(all("hydrogens" not in a for a in facts["graph"]["atoms"]))
                self.assertIn("smiles", analysis)

    def test_hydrogens_come_from_current_graph_not_cached_labels(self):
        doc = imported("CC")["document"]
        for atom in doc["atoms"]:
            atom["label_h"] = 99
        doc["bonds"][0]["order"] = 2
        result = handle(dict(protocol=1, operation="analyze", document=doc, local_properties=True))
        graph = result["analysis"]["property_input"]["graph"]
        self.assertEqual([a["explicit_hydrogens"] for a in graph["atoms"]], [0, 0])
        self.assertEqual(graph["bonds"], [dict(a=0, b=1, order=2, aromatic=False)])
        self.assertEqual([a["label_h"] for a in doc["atoms"]], [99, 99])

    def test_graph_hydrogens_and_isotopes_are_not_counted_as_attached_hydrogens(self):
        mol = Chem.AddHs(Chem.MolFromSmiles("[2H]O[3H]"))
        result = handle(
            dict(
                protocol=1,
                operation="import",
                format="mol",
                text=Chem.MolToMolBlock(mol),
                local_properties=True,
            )
        )
        atoms = result["analysis"]["property_input"]["graph"]["atoms"]
        self.assertEqual(sum(a["explicit_hydrogens"] for a in atoms), 0)
        self.assertEqual(sorted(a["isotope"] for a in atoms), [0, 2, 3])

    def test_local_analysis_never_reads_rdkit_hydrogen_totals(self):
        mol = Chem.MolFromSmiles("[13CH3][NH2+]Cc1cc[nH]c1")
        with (
            patch.object(Chem.Atom, "GetTotalNumHs", side_effect=AssertionError),
            patch.object(Chem.Atom, "GetNumImplicitHs", side_effect=AssertionError),
        ):
            result = analyze(mol, local_properties=True)
        self.assertTrue(result["property_input"]["graph"]["bonds"])
        self.assertNotIn("formula", result)

    def test_reference_mode_and_invalid_capabilities(self):
        request = dict(protocol=1, operation="import", format="smiles", text="CCO")
        reference = handle(request)
        self.assertEqual(reference, handle(dict(request, local_properties=False)))
        self.assertEqual(reference["analysis"]["formula"], "C2H6O")
        self.assertNotIn("property_input", reference["analysis"])
        for value in (1, "true", None, []):
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "boolean"):
                handle(dict(request, local_properties=value))


if __name__ == "__main__":
    unittest.main()
