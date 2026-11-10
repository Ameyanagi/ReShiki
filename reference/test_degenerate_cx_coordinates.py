import unittest

from degenerate_cx_coordinates import CoordinateInput
from rdkit import Chem, rdBase


class DegenerateCxCoordinatesTests(unittest.TestCase):
    def setUp(self):
        self.logs = rdBase.BlockLogs()

    def tearDown(self):
        del self.logs

    def test_coordinate_coverage_ignores_parentheses_in_text_fields(self):
        for suffix, counts in (
            ("|(0,0,),wU:0.4|", [1]),
            ("|$label()$,(),(0,0,)|", [0, 1]),
            ("|$_AV:label()$,()|", [0]),
            ("|atomProp:0.foo.(,()|", [0]),
            ("|atomProp:0.(.value():1.bar.(,()|", [0]),
            ("|SgD:0:name:data::::(a(b)),()|", [0]),
            ("|SgD:0:name():data()::::,()|", [0]),
            ("|SgD:0(name:data::::,()|", [0]),
            ("|Sg:n:0:label():hh,()|", [0]),
            ("|c():0,()|", [0]),
            ("|ctu(foo):0,()|", [0]),
            ("|LN:1:1.3.1(2,()|", [0]),
            ("|(1&#48;,0,)|", [1]),
            ("|(1&#48;,0,;)|", [1]),
            ("|(1&#;,0,;)|", [1]),
            ("|(;;)|", [2]),
            ("|()| name (ignored)", [0]),
        ):
            with self.subTest(suffix=suffix):
                self.assertEqual(
                    [n for _, _, n in CoordinateInput("CC " + suffix).sections], counts
                )

    def test_permissive_native_tag_boundaries_do_not_create_coordinate_sections(self):
        for section in ("c():0", "ctu(foo):0", "LN:1:1.3.1(2", "SgD:0(name:data::::"):
            with self.subTest(section=section):
                text = f"CC |{section},()|"
                mol = Chem.MolFromSmiles(text)
                self.assertIsNotNone(mol)
                self.assertTrue(CoordinateInput(text).drop(mol))
                self.assertEqual(mol.GetNumConformers(), 0)

    def test_only_empty_or_coincident_conformers_drop(self):
        for text, drops in (
            ("C |()|", True),
            ("C |(0,0,)|", False),
            ("CC |(0,0,)|", True),
            ("CC |(1,2,3;1,2,3)|", True),
            ("CC |(1,0,)|", False),
            ("CC |(nan,0,;nan,0,)|", False),
            ("CC |(inf,0,;inf,0,)|", True),
        ):
            with self.subTest(text=text):
                mol = Chem.MolFromSmiles(text)
                self.assertEqual(CoordinateInput(text).drop(mol), drops)
                self.assertEqual(mol.GetNumConformers(), 0 if drops else 1)

    def test_partial_blocks_drop_only_uncovered_singletons(self):
        coordinates = CoordinateInput("CO>N>C |(1,2,3)|")
        reactant = Chem.MolFromSmiles("CO |(1,2,3)|")
        agent = Chem.MolFromSmiles("N |(0,0,)|")
        product = Chem.MolFromSmiles("C |(0,0,)|")
        self.assertFalse(coordinates.drop(reactant))
        self.assertTrue(coordinates.drop(agent, atom_offset=2))
        self.assertTrue(coordinates.drop(product, atom_offset=3))

    def test_escape_terminators_do_not_cover_later_singletons(self):
        coordinates = CoordinateInput("C>O>N |(1&#;,0,;)|")
        singleton = Chem.MolFromSmiles("O |(0,0,)|")
        self.assertTrue(coordinates.drop(singleton, atom_offset=1))

    def test_remove_only_selected_sections_and_preserve_every_other_byte(self):
        text = "CC |$label()$,(),(1,2,3),(),w:0.0| name ()"
        self.assertEqual(
            CoordinateInput(text).without([0, 2]),
            "CC |$label()$, ,(1,2,3), ,w:0.0| name ()",
        )


if __name__ == "__main__":
    unittest.main()
