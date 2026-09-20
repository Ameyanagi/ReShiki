import copy
import unittest

from rdkit import Chem
from rdkit.Chem import rdChemReactions

from engine.worker import handle


def request(operation, **values):
    return handle(dict(protocol=1, operation=operation, **values))


class ReactionTests(unittest.TestCase):
    def drawing(self, text="CCO.O>Cl>CC=O.O"):
        return request("import", format="rsmi", text=text)["document"]

    def test_export_expands_coefficients_and_keeps_agents_separate(self):
        doc = self.drawing("O>Cl>O")
        doc["reactions"][0]["reactants"][0]["coefficient"] = 3
        original = copy.deepcopy(doc)
        for format in ("rsmi", "rxn"):
            output = request("export", format=format, document=doc)["output"]
            reaction = (
                rdChemReactions.ReactionFromSmarts(output, useSmiles=True)
                if format == "rsmi"
                else rdChemReactions.ReactionFromRxnBlock(output, sanitize=True)
            )
            self.assertEqual(reaction.GetNumReactantTemplates(), 3)
            self.assertEqual(reaction.GetNumProductTemplates(), 1)
            self.assertEqual(reaction.GetNumAgentTemplates(), 1)
            self.assertEqual(Chem.MolToSmiles(reaction.GetAgentTemplate(0)), "Cl")
        self.assertEqual(doc, original)

    def test_bad_membership_and_ambiguous_or_incomplete_exports_fail(self):
        doc = self.drawing()
        for defect in ("duplicate", "partial", "missing", "coefficient", "incomplete", "arrow"):
            bad = copy.deepcopy(doc)
            step = bad["reactions"][0]
            if defect == "duplicate":
                step["products"].append(copy.deepcopy(step["reactants"][0]))
            elif defect == "partial":
                step["reactants"][0]["atoms"].pop()
            elif defect == "missing":
                step["reactants"][0]["atoms"].append(100000)
            elif defect == "coefficient":
                step["reactants"][0]["coefficient"] = 0
            elif defect == "incomplete":
                step["products"] = []
            else:
                step["arrow"] = 100000
            with self.subTest(defect=defect), self.assertRaises(ValueError):
                request("export", format="rxn", document=bad)
        second = copy.deepcopy(doc["reactions"][0])
        second["arrow"] = 10000
        doc["arrows"].append(dict(id=10000, start=dict(x=0, y=0), end=dict(x=150, y=0)))
        doc["reactions"].append(second)
        with self.assertRaisesRegex(ValueError, "Choose one"):
            request("export", format="rxn", document=doc)
        output = request("export", format="rxn", document=doc, selected_ids=[10000])["output"]
        self.assertTrue(output.startswith("$RXN"))

    def test_mapping_is_preserved_and_duplicate_maps_are_rejected(self):
        doc = self.drawing("[CH3:1][OH:2]>O>[CH2:1]=[O:2]")
        original = request("export", format="rsmi", document=doc)["output"]
        rxn = request("export", format="rxn", document=doc)["output"]
        back = request("import", format="rxn", text=rxn)["document"]
        self.assertEqual(request("export", format="rsmi", document=back)["output"], original)
        doc["reactions"][0]["reactants"][0]["coefficient"] = 2
        with self.assertRaisesRegex(ValueError, "unique"):
            request("export", format="rxn", document=doc)

    def test_analysis_cleanup_and_abbreviation_replacement_keep_membership(self):
        doc = self.drawing("CCO>>CC=O")
        reaction = copy.deepcopy(doc["reactions"])
        self.assertEqual(request("analyze", document=doc)["document"]["reactions"], reaction)
        ids = reaction[0]["reactants"][0]["atoms"]
        cleaned = request(
            "clean",
            document=doc,
            selected_ids=ids,
            cleanup=dict(scope="selected_molecules", keep_orientation=True),
        )["document"]
        self.assertEqual(cleaned["reactions"], reaction)
        fixed = set(reaction[0]["products"][0]["atoms"])
        self.assertEqual(
            [a for a in cleaned["atoms"] if a["id"] in fixed],
            [a for a in doc["atoms"] if a["id"] in fixed],
        )
        replaced = request(
            "abbreviate", document=doc, selected_ids=[ids[-1]], format="replace", text="OMe"
        )["document"]
        output = request("export", format="rsmi", document=replaced)["output"]
        self.assertEqual(output, "CCOC>>CC=O")

    def test_real_rxn_queries_are_rejected_without_flattening(self):
        reaction = rdChemReactions.ReactionFromSmarts("[#6,#7]>>[#6]")
        text = rdChemReactions.ReactionToV3KRxnBlock(reaction)
        with self.assertRaisesRegex(ValueError, "Query"):
            request("import", format="rxn", text=text)


if __name__ == "__main__":
    unittest.main()
