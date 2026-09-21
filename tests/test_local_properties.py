"""The migrated worker supplies checked atom facts without calculating descriptors."""

import copy
import unittest
from unittest.mock import patch

from rdkit import Chem, rdBase

from engine.worker import handle


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
            ):
                result = handle(dict(protocol=1, local_properties=True, **copy.deepcopy(request)))
                analysis = result["analysis"]
                for key in ("formula", "mass", "exact_mass", "unpaired_electrons"):
                    self.assertNotIn(key, analysis)
                facts = analysis["property_input"]
                self.assertEqual(facts["rdkit_version"], rdBase.rdkitVersion)
                self.assertEqual(len(facts["atoms"]), len(result["document"]["atoms"]))
                self.assertIn("logp", analysis)

    def test_hydrogens_come_from_current_graph_not_cached_labels(self):
        doc = imported("CC")["document"]
        for atom in doc["atoms"]:
            atom["label_h"] = 99
        doc["bonds"][0]["order"] = 2
        result = handle(dict(protocol=1, operation="analyze", document=doc, local_properties=True))
        atoms = result["analysis"]["property_input"]["atoms"]
        self.assertEqual([a["hydrogens"] for a in atoms], [2, 2])
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
        atoms = result["analysis"]["property_input"]["atoms"]
        self.assertEqual(sum(a["hydrogens"] for a in atoms), 0)
        self.assertEqual(sorted(a["isotope"] for a in atoms), [0, 2, 3])

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
