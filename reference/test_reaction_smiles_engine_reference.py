import unittest
from unittest.mock import patch

import reaction_smiles_engine_reference as oracle
from rdkit import rdBase

from engine.worker import handle


def original_response(text):
    return handle(dict(protocol=1, operation="import", format="rsmi", text=text))


class ReactionSmilesEngineReferenceTests(unittest.TestCase):
    def setUp(self):
        self.logs = rdBase.BlockLogs()

    def tearDown(self):
        del self.logs

    def test_controls_keep_the_original_response(self):
        for text in (
            "CO>N>CC",
            "C>O>N |()|",  # Dropping singleton origins cannot change their drawing.
            "CO>N>C |(1,2,3)|",
            "CO>N>CC |(1,2,3;4,5,6;7,8,9;10,11,12;13,14,15)|",
            # Dropping an unused later conformer leaves the complete response intact.
            "CO>N>C |(1,2,3;4,5,6;7,8,9;10,11,12),()|",
        ):
            with self.subTest(text=text):
                original = original_response(text)
                with patch.object(oracle, "handle", return_value=original):
                    self.assertIs(oracle.reference_response(text), original)

    def test_supplied_reactant_geometry_survives_while_product_is_depicted(self):
        text = "CO>N>CC |(1,2,3;4,5,6)|"
        original = original_response(text)
        adapted = oracle.reference_response(text)
        before, after = original["document"], adapted["document"]
        self.assertEqual(after["atoms"][:3], before["atoms"][:3])
        self.assertEqual(after["reactions"], before["reactions"])
        self.assertEqual(after["bonds"], before["bonds"])
        self.assertEqual(after["atoms"][0]["position"], {"x": 36.0, "y": 42.0})
        self.assertEqual(after["atoms"][1]["position"], {"x": 120.0, "y": -42.0})
        self.assertEqual(before["atoms"][-1]["position"]["x"], 416.0)
        self.assertEqual(after["atoms"][-1]["position"]["x"], 458.0)

    def test_empty_sections_preserve_wavy_and_wedge_chemistry(self):
        for graph, section in (
            ("CC", "w:0.0"),
            ("F[C@](Cl)(Br)I", "wU:1.0"),
            ("F[C@](Cl)(Br)I", "wD:1.0"),
        ):
            with self.subTest(graph=graph, section=section):
                self.assertEqual(
                    oracle.reference_response(f"{graph}>O>N |(),(),{section}|"),
                    original_response(f"{graph}>O>N |{section}|"),
                )

    def test_dropped_sections_do_not_replace_retained_participant_geometry(self):
        expected = oracle.reference_response("CO>N>CC |(1,2,3;4,5,6),w:3.1|")
        actual = oracle.reference_response("CO>N>CC |(),(1,2,3;4,5,6),(),w:3.1|")
        self.assertEqual(actual, expected)
        self.assertEqual(actual["document"]["atoms"][0]["position"], {"x": 36.0, "y": 42.0})
        self.assertEqual(actual["document"]["bonds"][-1]["display"], "wavy")

    def test_regenerated_geometry_restores_all_five_logged_identifier_cases(self):
        cases = (
            (
                (
                    "CO>C/C=C/C>CC |(1,2,3;4,5,6)|",
                    "CO>C/C=C/C>CC |(1,2,,invalid)|",
                ),
                "C/C=C/C.CC.CO",
                "InChI=1S/C4H8.C2H6.CH4O/c1-3-4-2;2*1-2/h3-4H,1-2H3;1-2H3;2H,1H3/b4-3+;;",
                "IKQCXJRRHZMRIS-CZEFNJPISA-N",
            ),
            (
                ("C/C=C/C>N>CC |()|",),
                "C/C=C/C.CC.N",
                "InChI=1S/C4H8.C2H6.H3N/c1-3-4-2;1-2;/h3-4H,1-2H3;1-2H3;1H3/b4-3+;;",
                "KXZURTGLVBJKKY-CZEFNJPISA-N",
            ),
            (
                (
                    "F/C=C/F>F/C=C/F>F/C=C/F |(;;;;;;;;;;;;;1,2,3)|",
                    "F/C=C/F>F/C=C/F>F/C=C/F |(;;;;;;;;;;;;;;;bad)|",
                ),
                "F/C=C/F.F/C=C/F.F/C=C/F",
                "InChI=1S/3C2H2F2/c3*3-1-2-4/h3*1-2H/b3*2-1+",
                "PEPPETZVDBTTKK-ONWGBJBRSA-N",
            ),
        )
        for inputs, smiles, inchi, inchikey in cases:
            for text in inputs:
                with self.subTest(text=text):
                    original = original_response(text)
                    adapted = oracle.reference_response(text)
                    self.assertNotEqual(adapted, original)
                    self.assertEqual(
                        {key: adapted["analysis"][key] for key in ("smiles", "inchi", "inchikey")},
                        dict(smiles=smiles, inchi=inchi, inchikey=inchikey),
                    )

    def test_regenerated_geometry_restores_all_four_logged_wedge_cases(self):
        for text, bond, endpoints in (
            ("C>CC(F)Cl>F[C@](Cl)(Br)I |(;;;;;;;;;;;;;;;;1,2,3)|", 3, (8, 7)),
            ("C>CC(F)Cl>F[C@](Cl)(Br)I |(;;;;;;;;;;;;;;;;;;bad)|", 3, (8, 7)),
            ("*[C@](F)(Cl)Br>O>N |(),$ _AP1$|", 0, (2, 1)),
            ("*[C@](F)(Cl)Br>O>N |(),$_AP1$|", 1, (2, 3)),
        ):
            with self.subTest(text=text):
                original = original_response(text)["document"]["bonds"][bond]
                adapted = oracle.reference_response(text)["document"]["bonds"][bond]
                self.assertEqual(original["display"], "plain")
                self.assertEqual(adapted["display"], "hash")
                self.assertEqual((adapted["a"], adapted["b"]), endpoints)

    def test_rejected_inputs_keep_the_original_exception(self):
        for text in ("invalid", "C>>", "C>>O |(bad)|", "C>>O |(nan,inf,-inf)|"):
            with self.subTest(text=text):
                with self.assertRaises((ValueError, RuntimeError)) as original:
                    original_response(text)
                with self.assertRaises(type(original.exception)) as adapted:
                    oracle.reference_response(text)
                self.assertEqual(str(adapted.exception), str(original.exception))


if __name__ == "__main__":
    unittest.main()
