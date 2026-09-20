import copy
import math
import unittest
import xml.etree.ElementTree as ET

from engine import drawing_styles
from engine.worker import handle


def request(operation, **kwargs):
    return handle(dict(protocol=1, operation=operation, **kwargs))


class DrawingStyleTests(unittest.TestCase):
    def styled(self):
        result = request("import", format="smiles", text="c1ccccc1O")
        doc = result["document"]
        style = {
            **drawing_styles.DEFAULT,
            "name": "Presentation",
            "font_size_pt": 16,
            "bond_length_pt": 24,
            "bond_length_world": 70,
            "line_width_pt": 1,
            "bold_width_pt": 3,
            "bond_spacing_ratio": 0.22,
            "font_family": "Helvetica",
        }
        doc["drawing_style"] = style
        for atom in doc["atoms"]:
            for axis in ("x", "y"):
                atom["position"][axis] *= 70 / 42
        return doc, result["analysis"]["inchikey"]

    def test_editable_exchange_retains_physical_size_and_style(self):
        doc, identity = self.styled()
        for format in ("cdxml", "cdx"):
            with self.subTest(format=format):
                output = request("export", format=format, document=doc)["output"]
                if format == "cdxml":
                    root = ET.fromstring(output)
                    self.assertEqual(float(root.get("BondLength")), 24)
                    self.assertEqual(float(root.get("LineWidth")), 1)
                    self.assertEqual(float(root.get("LabelSize")), 16)
                result = request("import", format=format, text=output)
                back = result["document"]
                self.assertEqual(result["analysis"]["inchikey"], identity)
                for field in (
                    "bond_length_pt",
                    "line_width_pt",
                    "font_size_pt",
                    "bold_width_pt",
                    "font_family",
                    "bond_spacing_ratio",
                ):
                    self.assertEqual(back["drawing_style"][field], doc["drawing_style"][field])
                for before, after in zip(doc["atoms"], back["atoms"]):
                    for axis in ("x", "y"):
                        self.assertAlmostEqual(
                            before["position"][axis] - doc["atoms"][0]["position"][axis],
                            after["position"][axis] - back["atoms"][0]["position"][axis],
                            places=3,
                        )

    def test_analysis_cleanup_and_aromatic_display_keep_style(self):
        doc, identity = self.styled()
        analyzed = request("analyze", document=doc)["document"]
        self.assertEqual(analyzed["drawing_style"], doc["drawing_style"])
        cleaned = request("clean", document=doc)["document"]
        self.assertEqual(cleaned["drawing_style"], doc["drawing_style"])
        atoms = {a["id"]: a for a in cleaned["atoms"]}
        for bond in cleaned["bonds"]:
            a, b = atoms[bond["a"]]["position"], atoms[bond["b"]]["position"]
            self.assertAlmostEqual(math.hypot(a["x"] - b["x"], a["y"] - b["y"]), 70, places=2)
        aromatic = request("aromatic", document=doc, selected_ids=[a["id"] for a in doc["atoms"]])[
            "document"
        ]
        xml = request("export", document=aromatic, format="cdxml")["output"]
        back = request("import", format="cdxml", text=xml)
        self.assertEqual(back["analysis"]["inchikey"], identity)
        self.assertFalse(
            back["document"]["graphics"], "The aromatic circle remains owned by its molecule"
        )

    def test_bad_style_does_not_change_input_or_succeed(self):
        doc, _ = self.styled()
        for field, invalid in [
            ("line_width_pt", float("nan")),
            ("bond_length_world", 42),
            ("font_size_pt", -1),
            ("bond_spacing_ratio", 9),
        ]:
            damaged = copy.deepcopy(doc)
            damaged["drawing_style"][field] = invalid
            with self.assertRaises(ValueError):
                request("analyze", document=damaged)
        self.assertEqual(doc["drawing_style"]["line_width_pt"], 1)
