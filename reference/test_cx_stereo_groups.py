"""Keep the #1006 oracle exception limited to actual wrapping collisions."""

import unittest

from rdkit import Chem
from smiles_read_reference import cases, prepared_snapshot, restored_snapshot
from support.cx_stereo_groups import query_renumbering, renumber


class StereoGroupRenumbering(unittest.TestCase):
    def test_collisions_preserve_kind_order_members_and_original_ids(self):
        for extension, expected, identities in (
            ("o1:0,o2147483649:0", "o1:0,o2:0", {(1, 2): 2147483649}),
            ("o2147483649:0,o1:0", "o2:0,o1:0", {(1, 2): 2147483649}),
            (
                "o1:0,o2147483649:0,o2147483649:0",
                "o1:0,o2:0,o2:0",
                {(1, 2): 2147483649},
            ),
            ("a:0,&858993459:1", "a:0,&1:1", {(2, 1): 858993459}),
            (
                "o858993459:0,o3006477107:1",
                "o1:0,o2:1",
                {(1, 1): 858993459, (1, 2): 3006477107},
            ),
            (
                "o1:0,o2:1,o2147483649:0,o2147483649:1",
                "o1:0,o2:1,o3:0,o3:1",
                {(1, 3): 2147483649},
            ),
            ("o01:0,o2147483649:0", "o01:0,o2:0", {(1, 2): 2147483649}),
        ):
            with self.subTest(extension=extension):
                self.assertEqual(renumber(f"CC |{extension}|"), (f"CC |{expected}|", identities))
                self.assertIsNone(renumber(f"CC |{expected}|"))

    def test_noncollisions_and_other_cx_syntax_stay_strict(self):
        for text in (
            "CC |o2147483649:0|",
            "CC |o1:0,o1:0|",
            "CC |o1:0,o01:0|",
            "CC |o1:0,&2147483649:0|",
            "CC |o1:0,o4294967297:0|",
            "CC |o1:0,o2147483649:4294967296|",
            "CC |o1:0,o2147483649:0",
            "CC |o1:0,o2147483649:0,|",
            "CC |o1:0,,o2147483649:0|",
            "CC |$o1:0,o2147483649:0$|",
            "CC |o1:0,o2147483649:0,atomProp:0.x.y|",
        ):
            with self.subTest(text=text):
                self.assertIsNone(renumber(text))

    def test_query_exception_requires_a_changed_native_acceptance(self):
        text = 'M  V30 1 DAT 0 ATOMS=(1 1) QUERYTYPE=SMARTSQ QUERYOP="=" FIELDDATA="CC |o1:0,o2147483649:0|"\n'
        rewritten = text.replace("o2147483649", "o2")

        def reject(alternate):
            self.assertEqual(alternate, rewritten)
            raise ValueError("query contract")

        self.assertEqual(query_renumbering(text, True, reject, "query contract"), rewritten)
        self.assertIsNone(query_renumbering(text, False, reject, "query contract"))
        self.assertIsNone(query_renumbering(text, True, lambda _: None, "query contract"))
        self.assertIsNone(query_renumbering(text, True, reject, "another rejection"))
        self.assertIsNone(
            query_renumbering(text.replace("SMARTSQ", "OTHER"), True, reject, "query contract")
        )


class SmilesReadStereoGroups(unittest.TestCase):
    def test_corpus_renumberings_change_native_reading(self):
        corpus = {text for _, text in cases()}
        for text in (
            "CC |o1:0,o2147483649:0|",
            "CC |o2147483649:0,o1:0|",
            "[H][C@](F)(Cl)Br |o1:1,o2147483649:3|",
        ):
            with self.subTest(text=text):
                self.assertIn(text, corpus)
                normalized = renumber(text)
                self.assertIsNotNone(normalized)
                rewritten, identities = normalized
                self.assertEqual(identities, {(1, 2): 2147483649})
                mol = Chem.MolFromSmiles(rewritten)
                self.assertIsNotNone(mol)
                original = Chem.MolFromSmiles(text)
                self.assertNotEqual(
                    restored_snapshot(mol, text),
                    prepared_snapshot(original) if original is not None else None,
                )

    def test_snapshot_restores_large_read_id_on_retained_stereo_group(self):
        text = "F[C@H](Cl)[C@@H](Br)I |o1:1,o2147483649:3|"
        rewritten, _ = renumber(text)
        mol = Chem.MolFromSmiles(rewritten)
        self.assertIsNotNone(mol)
        before = prepared_snapshot(mol)
        self.assertEqual([g["read_id"] for g in before["metadata"]["groups"]], [1, 2])
        expected = prepared_snapshot(mol)
        expected["metadata"]["groups"][1]["read_id"] = 2147483649
        self.assertEqual(restored_snapshot(mol, text), expected)


if __name__ == "__main__":
    unittest.main()
