import copy
import json
from pathlib import Path
import subprocess
import sys
import unittest
import xml.etree.ElementTree as ET

from rdkit import Chem
from engine.worker import handle, from_document

ROOT = Path(__file__).resolve().parents[1]


def imported(smiles):
    return handle(dict(protocol=1, operation="import", format="smiles", text=smiles))


class ChemistryTests(unittest.TestCase):
    def test_molecule_identity_survives_editable_graph_and_cleanup(self):
        for smiles in ["CCO", "c1ccccc1", "CC(=O)Oc1ccccc1C(=O)O", "[Na+].[Cl-]",
                       "[13CH3]O", "N[C@@H](C)C(=O)O", "F[C@](Cl)(Br)I", "F/C=C/F", "F/C=C\\F"]:
            with self.subTest(smiles=smiles):
                initial = imported(smiles)
                doc = initial["document"]
                for op in ["analyze", "clean"]:
                    result = handle(dict(protocol=1, operation=op, document=doc))
                    self.assertEqual(initial["analysis"]["smiles"], result["analysis"]["smiles"])
                    self.assertEqual([a["id"] for a in doc["atoms"]], [a["id"] for a in result["document"]["atoms"]])

    def test_neighbor_order_is_not_stereochemistry(self):
        initial = imported("N[C@@H](C)C(=O)O")
        doc = copy.deepcopy(initial["document"])
        doc["atoms"].reverse()
        doc["bonds"].reverse()
        self.assertEqual(Chem.MolToSmiles(from_document(doc)), initial["analysis"]["smiles"])

    def test_cleanup_preserves_nonchemical_objects(self):
        doc = imported("CCO")["document"]
        for a in doc["atoms"]:
            a["position"]["x"] += 350
            a["position"]["y"] -= 170
        doc["annotations"] = [{"id":100,"position":{"x":5,"y":8},"text":"heat"}]
        doc["arrows"] = [{"id":101,"start":{"x":0,"y":0},"end":{"x":42,"y":0}}]
        result = handle(dict(protocol=1, operation="clean", document=doc))["document"]
        self.assertEqual(doc["annotations"], result["annotations"])
        self.assertEqual(doc["arrows"], result["arrows"])
        for axis in ["x", "y"]:
            self.assertAlmostEqual(sum(a["position"][axis] for a in doc["atoms"]),
                                   sum(a["position"][axis] for a in result["atoms"]))

    def test_invalid_valence_is_rejected(self):
        with self.assertRaises(ValueError):
            imported("C(C)(C)(C)(C)C")

    def test_mol_and_cdxml_round_trips(self):
        for smiles in ["CCO", "c1ccccc1", "N[C@@H](C)C(=O)O", "F/C=C/F", "[13CH3]O"]:
            initial = imported(smiles)
            for fmt in ["mol", "cdxml"]:
                with self.subTest(smiles=smiles, format=fmt):
                    output = handle(dict(protocol=1, operation="export", format=fmt, document=initial["document"]))["output"]
                    result = handle(dict(protocol=1, operation="import", format=fmt, text=output))
                    self.assertEqual(initial["analysis"]["smiles"], result["analysis"]["smiles"])

    def test_cdxml_preserves_supported_annotations(self):
        doc = imported("CCO")["document"]
        doc["annotations"] = [{"id":50,"position":{"x":0,"y":0},"text":"heat"}]
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))["output"]
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertEqual(result["document"]["annotations"][0]["text"], "heat")

    def test_reference_ui_export_imports_as_ethanol(self):
        xml = (ROOT / "tests/fixtures/reference-ethanol.cdxml").read_text()
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertEqual(result["analysis"]["smiles"], "CCO")
        self.assertEqual(result["analysis"]["formula"], "C2H6O")

    def test_cdxml_uses_publication_scale_without_changing_chemistry(self):
        doc = imported("CCO")["document"]
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))["output"]
        root = ET.fromstring(xml)
        self.assertAlmostEqual(float(root.attrib["BondLength"]), 14.4)
        self.assertAlmostEqual(float(root.attrib["LineWidth"]), 0.6)
        self.assertEqual(float(root.attrib["LabelSize"]), 10)
        nodes = root.findall(".//n")
        first, second = [[float(v) for v in n.attrib["p"].split()] for n in nodes[:2]]
        self.assertAlmostEqual(sum((a-b)**2 for a, b in zip(first, second))**0.5, 14.4, places=4)
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertEqual(result["analysis"]["smiles"], "CCO")
        for before, after in zip(doc["atoms"], result["document"]["atoms"]):
            # Import may translate, but must preserve relative geometry and scale.
            for axis in ("x", "y"):
                self.assertAlmostEqual(before["position"][axis] - doc["atoms"][0]["position"][axis],
                    after["position"][axis] - result["document"]["atoms"][0]["position"][axis], places=4)

    def test_freehand_ui_saved_drawing_and_exports(self):
        fixtures = ROOT / "tests/fixtures"
        doc = json.loads((fixtures / "ui-drawn-ethanol.moruno").read_text())
        self.assertEqual(Chem.MolToSmiles(from_document(doc)), "CCO")
        self.assertEqual(doc["annotations"][0]["text"], "oxidation")
        self.assertEqual(len(doc["arrows"]), 1)
        xml = (fixtures / "ui-drawn-ethanol.cdxml").read_text()
        self.assertEqual(Chem.MolToSmiles(Chem.MolsFromCDXML(xml)[0]), "CCO")
        self.assertEqual(len(ET.fromstring(xml).findall(".//arrow")), 1)
        roundtrip = handle(dict(protocol=1,operation="import",format="cdxml",text=xml))["document"]
        self.assertEqual(roundtrip["annotations"][0]["text"], "oxidation")
        self.assertEqual(roundtrip["arrows"][0]["kind"], "forward")
        original_delta = doc["arrows"][0]["start"]["x"] - doc["atoms"][0]["position"]["x"]
        imported_delta = roundtrip["arrows"][0]["start"]["x"] - roundtrip["atoms"][0]["position"]["x"]
        self.assertAlmostEqual(original_delta, imported_delta, places=3)
        svg = ET.parse(fixtures / "ui-drawn-ethanol.svg")
        self.assertIn("oxidation", "".join(svg.getroot().itertext()))

    def test_worker_recovers_after_bad_request_and_keeps_ids(self):
        requests = ["not json", "[]", json.dumps(dict(id=2,protocol=1,operation="import",format="smiles",text="CCO"))]
        proc = subprocess.run([sys.executable, str(ROOT/"engine/worker.py")],
                              input="\n".join(requests)+"\n", capture_output=True, text=True, timeout=15)
        self.assertEqual(proc.returncode,0,proc.stderr)
        responses = [json.loads(line) for line in proc.stdout.splitlines()]
        self.assertFalse(responses[0]["ok"])
        self.assertFalse(responses[1]["ok"])
        self.assertTrue(responses[2]["ok"])
        self.assertEqual(responses[2]["id"],2)
        self.assertEqual(responses[2]["result"]["analysis"]["formula"],"C2H6O")


if __name__ == "__main__":
    unittest.main()
