import copy
import math
import unittest

from engine.worker import handle


def imported(smiles):
    return handle(dict(protocol=1, operation="import", format="smiles", text=smiles))


def clean(doc, scope="drawing", ids=None, orientation=True):
    return handle(
        dict(
            protocol=1,
            operation="clean",
            document=doc,
            selected_ids=ids or [],
            cleanup=dict(scope=scope, keep_orientation=orientation),
        )
    )


def distort(doc, ids, dx=0, dy=0):
    for a in doc["atoms"]:
        if a["id"] in ids:
            p = a["position"]
            x, y = p["x"], p["y"]
            p.update(x=1.7 * x + 0.4 * y + dx, y=0.8 * y + dy)


def centroid(doc, ids):
    points = [a["position"] for a in doc["atoms"] if a["id"] in ids]
    return tuple(sum(p[k] for p in points) / len(points) for k in ("x", "y"))


class Cleanup(unittest.TestCase):
    def test_independent_molecule_centers_and_document_objects_are_preserved(self):
        d = imported("CCO.c1ccccc1")["document"]
        first = {1, 2, 3}
        second = set(range(4, 10))
        distort(d, first, -320, 210)
        distort(d, second, 550, -110)
        d["annotations"] = [dict(id=10, position=dict(x=0, y=0), text="conditions", format={})]
        d["arrows"] = [dict(id=11, start=dict(x=-50, y=0), end=dict(x=100, y=0), kind="forward")]
        d["graphics"] = [
            dict(id=12, kind="rectangle", points=[dict(x=0, y=0), dict(x=30, y=20)], style={})
        ]
        d["groups"] = [dict(id=13, members=[1, 2, 3, 10], integral=True)]
        original = copy.deepcopy(d)
        result = clean(d)["document"]
        for ids in (first, second):
            for x, y in zip(centroid(d, ids), centroid(result, ids)):
                self.assertAlmostEqual(x, y, places=5)
        for key in ("annotations", "arrows", "graphics", "groups"):
            self.assertEqual(result[key], d[key])
        for bond in result["bonds"]:
            a = next(a["position"] for a in result["atoms"] if a["id"] == bond["a"])
            b = next(a["position"] for a in result["atoms"] if a["id"] == bond["b"])
            self.assertAlmostEqual(math.hypot(a["x"] - b["x"], a["y"] - b["y"]), 42, places=5)
        self.assertEqual(d, original)

    def test_partial_selection_locks_ring_and_unrelated_structure_exactly(self):
        d = imported("c1ccccc1CCC.CCO")["document"]
        moving = {7, 8, 9}
        distort(d, moving, 80, 70)
        original = copy.deepcopy(d)
        result = clean(d, "selected_atoms", list(moving))["document"]
        for old, new in zip(d["atoms"], result["atoms"]):
            if old["id"] not in moving:
                self.assertEqual(old, new)
        for old, new in zip(d["bonds"], result["bonds"]):
            if not {old["a"], old["b"]} & moving:
                self.assertEqual(old, new)
        self.assertTrue(
            any(
                a["position"] != b["position"]
                for a, b in zip(d["atoms"], result["atoms"])
                if a["id"] in moving
            )
        )
        self.assertEqual(d, original)
        self.assertEqual(
            clean(result, "selected_atoms", list(moving))["analysis"]["formula"], "C11H18O"
        )

    def test_selected_molecules_expands_to_component_without_touching_others(self):
        d = imported("c1ccccc1CCC.CCO")["document"]
        distort(d, set(range(1, 10)))
        result = clean(d, "selected_molecules", [7])["document"]
        self.assertTrue(
            any(
                a["position"] != b["position"]
                for a, b in zip(d["atoms"], result["atoms"])
                if a["id"] <= 6
            )
        )
        self.assertEqual(d["atoms"][9:], result["atoms"][9:])
        self.assertEqual(d["bonds"][9:], result["bonds"][9:])
        for x, y in zip(centroid(d, set(range(1, 10))), centroid(result, set(range(1, 10)))):
            self.assertAlmostEqual(x, y, places=5)

    def test_selected_cleanup_works_with_an_invalid_unselected_molecule(self):
        d = imported("CCO.CC")["document"]
        d["atoms"][3]["explicit_h"] = 5
        unchanged = copy.deepcopy(d["atoms"][3:])
        distort(d, {1, 2, 3})
        result = clean(d, "selected_molecules", [1])
        self.assertIsNone(result["analysis"])
        self.assertTrue(result["warnings"])
        self.assertEqual(result["document"]["atoms"][3:], unchanged)
        with self.assertRaises(ValueError):
            clean(d)

    def test_scope_validation_and_abbreviation_selection_are_atomic(self):
        d = imported("COc1ccc(NC(=O)OC(C)(C)C)cc1")["document"]
        grouped = handle(dict(protocol=1, operation="abbreviate", document=d))["document"]
        group = next(g for g in grouped["abbreviations"] if g["label"] == "Boc")
        result = clean(grouped, "selected_atoms", [group["anchor"]])["document"]
        self.assertEqual(result["abbreviations"], grouped["abbreviations"])
        for a, b in zip(grouped["atoms"], result["atoms"]):
            if a["id"] not in group["members"]:
                self.assertEqual(a, b)
        old = copy.deepcopy(grouped)
        for scope, ids in [
            ("invalid", [1]),
            ("selected_atoms", []),
            ("selected_molecules", [99999]),
        ]:
            with self.assertRaises(ValueError):
                clean(grouped, scope, ids)
        self.assertEqual(grouped, old)

    def test_partial_cleanup_preserves_absolute_and_visible_stereo(self):
        for smiles in ["F/C=C/C[C@H](Cl)Br", "N[C@@H](C)C(=O)O", "F[C@](Cl)(Br)I"]:
            d = imported(smiles)["document"]
            # All but one atom: verify an exact pin as well as stereo retention.
            ids = [a["id"] for a in d["atoms"][1:]]
            expected = handle(dict(protocol=1, operation="analyze", document=d))["analysis"][
                "smiles"
            ]
            result = clean(d, "selected_atoms", ids)
            self.assertEqual(result["analysis"]["smiles"], expected)
            self.assertEqual(result["document"]["atoms"][0], d["atoms"][0])
            drawn = copy.deepcopy(result["document"])
            for a in drawn["atoms"]:
                a["stereo"] = None
            for b in drawn["bonds"]:
                b["stereo"] = None
                b["stereo_atoms"] = []
            self.assertEqual(
                handle(dict(protocol=1, operation="analyze", document=drawn))["analysis"]["smiles"],
                expected,
            )

    def test_cleanup_does_not_assign_unknown_alkene_stereochemistry(self):
        d = imported("CCC=CCC")["document"]
        for i, a in enumerate(d["atoms"]):
            a["position"] = dict(x=50 * i, y=0)
        before = handle(dict(protocol=1, operation="analyze", document=d))["analysis"]["smiles"]
        result = clean(d)
        self.assertEqual(result["analysis"]["smiles"], before)
        self.assertEqual(
            next(b for b in result["document"]["bonds"] if b["order"] == 2)["display"], "wavy"
        )
        self.assertTrue(result["warnings"])

    def test_orientation_uses_rotation_without_reflection_or_rescaling(self):
        d = imported("CCO")["document"]
        angle = 0.81
        for a in d["atoms"]:
            p = a["position"]
            x, y = p["x"], p["y"]
            p.update(
                x=x * math.cos(angle) - y * math.sin(angle) + 70,
                y=x * math.sin(angle) + y * math.cos(angle) - 140,
            )
        result = clean(d)["document"]
        for a, b in zip(d["atoms"], result["atoms"]):
            self.assertAlmostEqual(a["position"]["x"], b["position"]["x"], places=5)
            self.assertAlmostEqual(a["position"]["y"], b["position"]["y"], places=5)
        rotated = clean(d, orientation=False)["document"]
        self.assertNotEqual(rotated["atoms"][0]["position"], d["atoms"][0]["position"])
        for x, y in zip(centroid(d, {1, 2, 3}), centroid(rotated, {1, 2, 3})):
            self.assertAlmostEqual(x, y, places=5)


if __name__ == "__main__":
    unittest.main()
