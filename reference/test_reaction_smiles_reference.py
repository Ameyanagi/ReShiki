import unittest

from rdkit import rdBase
from reaction_smiles_reference import read


class ReactionSmilesReferenceTests(unittest.TestCase):
    def setUp(self):
        self.logs = rdBase.BlockLogs()

    def tearDown(self):
        del self.logs

    def test_degenerate_geometry_leaves_reaction_wavy_and_wedge_state_intact(self):
        for graph, section in (
            ("CC", "w:0.0"),
            ("F[C@](Cl)(Br)I", "wU:1.0"),
            ("F[C@](Cl)(Br)I", "wD:1.0"),
            ("C/C=C/C", "ctu:1"),
        ):
            with self.subTest(graph=graph, section=section):
                self.assertEqual(
                    read(f"{graph}>O>N |(),(),{section}|"),
                    read(f"{graph}>O>N |{section}|"),
                )
        self.assertEqual(
            read("CC>O>N |(),(),w:0.0|")["reactants"][0]["prepared"]["state"]["directions"],
            ["unknown"],
        )

    def test_partial_coordinates_keep_the_reactant_and_drop_later_participants(self):
        actual = read("CO>N>CC |(1,2,3;4,5,6)|")
        self.assertEqual(
            actual["reactants"][0]["conformers"],
            [dict(positions=[dict(x=1.0, y=2.0, z=3.0), dict(x=4.0, y=5.0, z=6.0)], is_3d=True)],
        )
        self.assertEqual(actual["agents"][0]["conformers"], [])
        self.assertEqual(actual["products"][0]["conformers"], [])
        for role, parts in read("CO>N>CC").items():
            for index, part in enumerate(parts):
                self.assertEqual(actual[role][index]["prepared"], part["prepared"])

    def test_global_coverage_uses_reactants_then_agents_then_products(self):
        actual = read("C.O>N.C>CC |(0,0,;1,0,;2,0,;3,0,)|")
        for role, xs in (("reactants", (0.0, 1.0)), ("agents", (2.0, 3.0))):
            for part, x in zip(actual[role], xs, strict=True):
                self.assertEqual(
                    part["conformers"],
                    [dict(positions=[dict(x=x, y=0.0, z=0.0)], is_3d=False)],
                )
        self.assertEqual(actual["products"][0]["conformers"], [])

    def test_repeated_coordinate_sections_preserve_the_retained_conformer(self):
        self.assertEqual(
            read("CO>N>CC |(),(1,2,3;4,5,6),()|"),
            read("CO>N>CC |(1,2,3;4,5,6)|"),
        )

    def test_explicit_hydrogens_and_nonfinite_geometry_remain_visible(self):
        for coords in ("0,0,;1,0,", "nan,inf,-inf"):
            with self.subTest(coords=coords):
                part = read(f"[H]C>O>N |({coords})|")["reactants"][0]
                self.assertEqual(len(part["prepared"]["state"]["graph"]["atoms"]), 2)
                self.assertEqual(len(part["conformers"]), 1)
        part = read("[H]C>O>N |(nan,inf,-inf)|")["reactants"][0]
        self.assertEqual(part["conformers"][0]["positions"][0], dict(x=None, y=None, z=None))


if __name__ == "__main__":
    unittest.main()
