import copy
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from rdkit import Chem

from engine.worker import from_document, handle


def call(op, **kwargs):
    return handle(dict(protocol=1, operation=op, **kwargs))


class AromaticTests(unittest.TestCase):
    def test_toggle_exchange_cleanup_and_identity_of_heterocycles_and_fused_rings(self):
        for smiles in ("c1ccccc1", "c1ccoc1", "c1cc[nH]c1", "c1ncc[nH]1", "c1ccc2ccccc2c1"):
            with self.subTest(smiles=smiles):
                original = call("import", format="smiles", text=smiles)
                doc = original["document"]
                ids = [a["id"] for a in doc["atoms"]]
                circle = call("aromatic", document=doc, selected_ids=ids)
                self.assertEqual(circle["analysis"]["inchikey"], original["analysis"]["inchikey"])
                self.assertTrue(all(b["order"] == 4 for b in circle["document"]["bonds"]))
                for a, b in zip(doc["atoms"], circle["document"]["atoms"]):
                    self.assertEqual(a["position"], b["position"])
                for fmt in ("cdxml", "cdx"):
                    exported = call("export", document=circle["document"], format=fmt)
                    back = call("import", format=fmt, text=exported["output"])
                    self.assertEqual(back["analysis"]["inchikey"], original["analysis"]["inchikey"])
                    self.assertEqual(back["document"]["graphics"], [])
                    self.assertTrue(all(b["order"] == 4 for b in back["document"]["bonds"]))
                cleaned = call(
                    "clean",
                    document=circle["document"],
                    selected_ids=ids,
                    cleanup=dict(scope="selected_atoms", keep_orientation=True),
                )
                self.assertTrue(all(b["order"] == 4 for b in cleaned["document"]["bonds"]))
                back = call("aromatic", document=circle["document"], selected_ids=ids)
                self.assertTrue(all(b["order"] in (1, 2) for b in back["document"]["bonds"]))
                self.assertEqual(back["analysis"]["inchikey"], original["analysis"]["inchikey"])

    def test_selected_ring_only_and_invalid_selection_is_nonmutating(self):
        doc = call("import", format="smiles", text="Cc1ccccc1.CCO")["document"]
        original = copy.deepcopy(doc)
        ids = list(range(2, 8))
        result = call("aromatic", document=doc, selected_ids=ids)["document"]
        self.assertEqual(doc, original)
        for before, after in zip(doc["bonds"], result["bonds"]):
            if before["a"] not in ids or before["b"] not in ids:
                self.assertEqual(before, after)
        self.assertEqual(doc["atoms"], result["atoms"])
        for selection in ([], [2, 3], [8, 9, 10]):
            with self.assertRaisesRegex(ValueError, "Select"):
                call("aromatic", document=doc, selected_ids=selection)
        saturated = call("import", format="smiles", text="C1CCCCC1")["document"]
        with self.assertRaisesRegex(ValueError, "saturated"):
            call("aromatic", document=saturated, selected_ids=list(range(1, 7)))

    def test_native_fixture_has_owned_circle_and_ordinary_ovals_are_retained(self):
        text = (Path(__file__).parent / "fixtures/aromatic-circle-native.cdxml").read_text()
        result = call("import", format="cdxml", text=text)
        self.assertEqual(result["analysis"]["formula"], "C6H6")
        self.assertEqual(result["document"]["graphics"], [])
        root = ET.fromstring(text)
        oval = copy.deepcopy(next(root.iter("graphic")))
        oval.set("id", "999")
        root.find("page").append(oval)
        result = call("import", format="cdxml", text=ET.tostring(root, encoding="unicode"))
        self.assertEqual(len(result["document"]["graphics"]), 1)

    def test_partial_fused_ring_changes_display_without_changing_identity(self):
        doc = call("import", format="smiles", text="c1ccc2ccccc2c1")["document"]
        mol = from_document(doc)
        ring = mol.GetRingInfo().AtomRings()[0]
        ids = [doc["atoms"][i]["id"] for i in ring]
        circle = call("aromatic", document=doc, selected_ids=ids)
        self.assertEqual(circle["analysis"]["smiles"], Chem.MolToSmiles(mol))
        back = call("aromatic", document=circle["document"], selected_ids=ids)
        self.assertEqual(back["analysis"]["smiles"], Chem.MolToSmiles(mol))


if __name__ == "__main__":
    unittest.main()
