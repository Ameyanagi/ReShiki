import io
import json
import unittest
from contextlib import redirect_stdout
from unittest.mock import patch

import smiles_read_reference as oracle
from rdkit import Chem, rdBase


class SmilesReadReferenceTests(unittest.TestCase):
    def setUp(self):
        self.logs = rdBase.BlockLogs()

    def tearDown(self):
        del self.logs

    def adapted(self, text):
        return oracle.without_degenerate_coordinates(text, Chem.MolFromSmiles(text))

    def test_coordinate_removal_keeps_neighboring_annotation_targets_separate(self):
        for text, directions, radicals in (
            ("CCC |w:1.0()1|", ["unknown", "none"], [0, 0, 0]),
            ("CCC |^1:0()1|", ["none", "none"], [1, 0, 0]),
            ("CCC |w:0.0()1|", ["unknown", "none"], [0, 0, 0]),
        ):
            with self.subTest(text=text):
                adapted = self.adapted(text)
                self.assertEqual(oracle.prepared_snapshot(adapted)["directions"], directions)
                self.assertEqual(
                    [atom.GetNumRadicalElectrons() for atom in adapted.GetAtoms()], radicals
                )
                self.assertEqual(adapted.GetNumConformers(), 0)

    def test_repeated_empty_coordinates_do_not_consume_the_wavy_direction(self):
        text = "CC |(),(),w:0.0|"
        original = Chem.MolFromSmiles(text)
        adapted = self.adapted(text)
        self.assertEqual(oracle.prepared_snapshot(original)["directions"], ["none"])
        self.assertEqual(oracle.prepared_snapshot(adapted)["directions"], ["unknown"])
        self.assertEqual(adapted.GetNumConformers(), 0)

        # Exercise main's actual JSON transport, including the untouched state
        # projection used by the independent stereo-group PR.
        output = io.StringIO()
        with patch.object(oracle, "cases", return_value=[("regression", text)]):
            with redirect_stdout(output):
                oracle.main()
        case = json.loads(output.getvalue().splitlines()[1])
        self.assertIsNone(case["failure"])
        self.assertEqual(case["text"], text)
        self.assertEqual(case["expected"]["conformers"], [])
        self.assertEqual(case["expected"]["prepared"]["state"]["directions"], ["unknown"])

    def test_all_chemistry_comes_from_the_equivalent_input(self):
        for graph, flags in (
            ("CC", "w:0.0"),
            ("F[C@](Cl)(Br)I", "wU:1.0"),
            ("F[C@](Cl)(Br)I", "wD:1.0"),
            ("F/C=C/F", "w:1.1"),
            ("F/C=C/F", "wU:1.0"),
            ("[H][C@](F)(Cl)Br", "wD:1.1"),
        ):
            with self.subTest(graph=graph, flags=flags):
                adapted = self.adapted(f"{graph} |(),(0,0,),{flags}| named")
                equivalent = Chem.MolFromSmiles(f"{graph} |{flags}| named")
                self.assertEqual(
                    oracle.prepared_snapshot(adapted), oracle.prepared_snapshot(equivalent)
                )
                self.assertEqual(Chem.MolToCXSmiles(adapted), Chem.MolToCXSmiles(equivalent))
                self.assertEqual(adapted.GetNumConformers(), 0)
                self.assertEqual(adapted.GetProp("_Name"), "named")

    def test_labels_properties_and_name_survive_coordinate_removal(self):
        text = "C* |(),$;label()$,atomProp:0.foo.(| 名前 (kept)"
        adapted = self.adapted(text)
        equivalent = Chem.MolFromSmiles("C* |$;label()$,atomProp:0.foo.(| 名前 (kept)")
        self.assertEqual(oracle.prepared_snapshot(adapted), oracle.prepared_snapshot(equivalent))
        self.assertEqual(adapted.GetAtomWithIdx(0).GetProp("foo"), "(")
        self.assertEqual(adapted.GetAtomWithIdx(1).GetProp("atomLabel"), "label()")
        self.assertEqual(adapted.GetProp("_Name"), "名前 (kept)")

    def test_retained_coordinate_section_supplies_the_full_chemistry(self):
        retained = "(0,0,;1,0,;0,1,;-1,-1,;1,1,)"
        for discarded in ("()", "(0,0,)"):
            with self.subTest(discarded=discarded):
                adapted = self.adapted(f"F[C@](Cl)(Br)I |{discarded},{retained},wU:1.0|")
                equivalent = Chem.MolFromSmiles(f"F[C@](Cl)(Br)I |{retained},wU:1.0|")
                self.assertEqual(
                    oracle.prepared_snapshot(adapted), oracle.prepared_snapshot(equivalent)
                )
                self.assertEqual(adapted.GetNumConformers(), 1)
                self.assertEqual(
                    adapted.GetConformer().GetPositions().tolist(),
                    equivalent.GetConformer().GetPositions().tolist(),
                )

    def test_non_dropped_inputs_keep_the_original_molecule(self):
        for text in (
            "C",
            "C |(0,0,)|",
            "CC |(1,0,)|",
            "CC |(nan,0,;nan,0,)|",
            "[H][C@](F)(Cl)Br |(nan,inf,-inf)|",
            "[H]C(F)=C(Cl)[H] |(nan,inf,-inf)|",
        ):
            with self.subTest(text=text):
                original = Chem.MolFromSmiles(text)
                self.assertIs(oracle.without_degenerate_coordinates(text, original), original)

    def test_coincidence_is_decided_before_removing_explicit_hydrogens(self):
        text = "[H]C |(0,0,)|"
        self.assertEqual(Chem.MolFromSmiles(text).GetNumAtoms(), 1)
        self.assertEqual(self.adapted(text).GetNumConformers(), 0)

    def test_earlier_cx_normalization_is_preserved_when_reading_again(self):
        # #1006 renumbers CX stereo-group keys before main's initial RDKit
        # read, then restores their identities in the final state projection.
        text = "F[C@](Cl)(Br)I |(),o1:1| named"
        normalized = Chem.MolFromSmiles("F[C@](Cl)(Br)I |(),o7:1| named")
        adapted = oracle.without_degenerate_coordinates(text, normalized)
        equivalent = Chem.MolFromSmiles("F[C@](Cl)(Br)I |o7:1| named")
        self.assertEqual(oracle.prepared_snapshot(adapted), oracle.prepared_snapshot(equivalent))
        self.assertEqual(adapted.GetStereoGroups()[0].GetReadId(), 7)
        self.assertEqual(adapted.GetProp("_Name"), "named")


if __name__ == "__main__":
    unittest.main()
