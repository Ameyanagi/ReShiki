import copy
import json
import subprocess
import sys
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from rdkit import Chem

from engine.worker import TEXT_DEFAULTS, from_document, handle, text_runs

ROOT = Path(__file__).resolve().parents[1]


def imported(smiles):
    return handle(dict(protocol=1, operation="import", format="smiles", text=smiles))


class ChemistryTests(unittest.TestCase):
    def test_atom_indicators_from_chemdraw_remain_owned_and_keep_identity(self):
        result = handle(
            dict(
                protocol=1,
                operation="import",
                format="cdxml",
                text=(ROOT / "tests/fixtures/atom-labels-chemdraw.cdxml").read_text(
                    encoding="utf-8"
                ),
            )
        )
        doc = result["document"]
        self.assertEqual(result["analysis"]["formula"], "C9H11NO2")
        self.assertEqual(doc["annotations"], [])
        self.assertEqual(
            [a["display"]["number"]["text"] for a in doc["atoms"]], list(map(str, range(1, 13)))
        )
        self.assertTrue(all(not a["display"]["hydrogens"] for a in doc["atoms"]))
        self.assertEqual([a["cip_label"] for a in doc["atoms"] if a["cip_label"]], ["S"])
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))["output"]
        root = ET.fromstring(xml)
        self.assertEqual(len(list(root.iter("objecttag"))), 13)
        back = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertEqual(back["analysis"]["inchikey"], result["analysis"]["inchikey"])
        self.assertEqual(back["document"]["annotations"], [])
        self.assertEqual(float(root.get("BondLength")), 14.4)
        self.assertEqual(float(root.get("LabelSize")), 10.0)

    def test_cip_is_recomputed_instead_of_trusting_imported_text(self):
        xml = (
            (ROOT / "tests/fixtures/atom-labels-chemdraw.cdxml")
            .read_text(encoding="utf-8")
            .replace("(S)", "(R)")
        )
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertEqual(
            [a["cip_label"] for a in result["document"]["atoms"] if a["cip_label"]], ["S"]
        )
        for smiles, label in [("F/C=C/F", "E"), ("F/C=C\\F", "Z")]:
            result = imported(smiles)
            self.assertEqual(
                [b["cip_label"] for b in result["document"]["bonds"] if b["cip_label"]], [label]
            )

    def test_unknown_or_detached_object_tags_are_rejected(self):
        xml = (ROOT / "tests/fixtures/atom-labels-chemdraw.cdxml").read_text(encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "object tag"):
            handle(
                dict(
                    protocol=1,
                    operation="import",
                    format="cdxml",
                    text=xml.replace('Name="number"', 'Name="custom"'),
                )
            )

    def test_worker_recovers_from_malformed_graph_and_large_integer(self):
        good = dict(protocol=1, operation="import", format="smiles", text="CCO")
        doc = imported("CC")["document"]
        bad = copy.deepcopy(doc)
        bad["bonds"][0]["a"] = 999
        huge = copy.deepcopy(doc)
        huge["atoms"][0]["charge"] = 10**100
        requests = [
            dict(protocol=1, operation="analyze", document=bad),
            dict(protocol=1, operation="analyze", document=huge),
            good,
        ]
        proc = subprocess.run(
            [sys.executable, str(ROOT / "engine/worker.py")],
            input="\n".join(json.dumps(r) for r in requests) + "\n",
            capture_output=True,
            text=True,
            timeout=15,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        responses = [json.loads(line) for line in proc.stdout.splitlines()]
        self.assertEqual([r["ok"] for r in responses], [False, False, True])
        self.assertEqual(responses[-1]["result"]["analysis"]["formula"], "C2H6O")

    def test_chemdraw_saved_symbols_keep_atom_ownership_and_jacs_defaults(self):
        xml = (ROOT / "tests/fixtures/attached-symbols-chemdraw.cdxml").read_text(encoding="utf-8")
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertEqual(result["analysis"]["formula"], "H4N+")
        doc = result["document"]
        self.assertEqual(doc["graphics"], [])
        self.assertEqual(doc["atoms"][0]["marks"][0]["kind"], "circled_charge")
        output = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))[
            "output"
        ]
        root = ET.fromstring(output)
        self.assertEqual(float(root.get("BondLength")), 14.4)
        self.assertEqual(float(root.get("LineWidth")), 0.6)
        self.assertEqual(float(root.get("LabelSize")), 10.0)
        again = handle(dict(protocol=1, operation="import", format="cdxml", text=output))
        self.assertEqual(again["analysis"]["smiles"], "[NH4+]")
        self.assertAlmostEqual(again["document"]["atoms"][0]["marks"][0]["size_pt"], 7.5, places=3)
        free = handle(
            dict(
                protocol=1,
                operation="import",
                format="cdxml",
                text=(ROOT / "tests/fixtures/symbols-chemdraw.cdxml").read_text(encoding="utf-8"),
            )
        )["document"]
        self.assertEqual(
            [g["kind"] for g in free["graphics"]], [{"orbital": "p"}, {"symbol": "circle_plus"}]
        )
        self.assertEqual(free["graphics"][0]["phase"], "open")
        self.assertEqual(free["atoms"], [])

    def test_radical_identity_survives_chemical_formats_and_cleanup(self):
        for smiles, count in [("[CH3]", 1), ("[CH2]", 2), ("[NH3+]", 1)]:
            with self.subTest(smiles=smiles):
                result = imported(smiles)
                doc = result["document"]
                self.assertEqual(result["analysis"]["unpaired_electrons"], count)
                clean = handle(dict(protocol=1, operation="clean", document=doc))
                self.assertEqual(clean["analysis"]["smiles"], smiles)
                for fmt in ("mol", "cdxml"):
                    output = handle(dict(protocol=1, operation="export", format=fmt, document=doc))[
                        "output"
                    ]
                    restored = handle(dict(protocol=1, operation="import", format=fmt, text=output))
                    self.assertEqual(restored["analysis"]["smiles"], smiles)
                    self.assertEqual(restored["analysis"]["unpaired_electrons"], count)
        doc = imported("C")["document"]
        doc["atoms"][0]["radical_electrons"] = 1
        doc["atoms"][0]["explicit_h"] = 4
        doc["atoms"][0]["no_implicit"] = True
        with self.assertRaises(ValueError):
            handle(dict(protocol=1, operation="analyze", document=doc))

    def test_unsupported_attached_symbols_fail_without_silent_chemical_changes(self):
        doc = imported("C")["document"]
        doc["atoms"][0]["marks"] = [dict(kind="lone_pair", offset={"x": 0, "y": -20})]
        with self.assertRaisesRegex(ValueError, "carbon lone-pair"):
            handle(dict(protocol=1, operation="export", format="cdxml", document=doc))
        doc["atoms"][0]["marks"][0]["kind"] = "lone_pair_bar"
        with self.assertRaisesRegex(ValueError, "native/SVG/PDF/PNG"):
            handle(dict(protocol=1, operation="export", format="cdxml", document=doc))
        xml = (
            (ROOT / "tests/fixtures/symbols-chemdraw.cdxml")
            .read_text(encoding="utf-8")
            .replace('OrbitalType="p"', 'OrbitalType="pShaded"')
        )
        with self.assertRaisesRegex(ValueError, "Gradient-shaded"):
            handle(dict(protocol=1, operation="import", format="cdxml", text=xml))

    def test_cdxml_attached_charges_keep_the_reference_editors_proximity_contract(self):
        doc = imported("[NH4+]")["document"]
        doc["atoms"][0]["explicit_h"] = 0
        doc["atoms"][0]["no_implicit"] = False
        doc["atoms"][0]["marks"] = [
            dict(kind="circled_charge", offset={"x": 0, "y": -10 * 42 / 14.4})
        ]
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))["output"]
        node = ET.fromstring(xml).find(".//n")
        self.assertEqual(node.get("NumHydrogens"), "4")
        self.assertIsNotNone(node.find("t"))
        saved = handle(
            dict(
                protocol=1,
                operation="import",
                format="cdxml",
                text=(ROOT / "tests/fixtures/exported-charge-chemdraw.cdxml").read_text(
                    encoding="utf-8"
                ),
            )
        )
        self.assertEqual(saved["analysis"]["smiles"], "[NH4+]")
        self.assertEqual(saved["document"]["atoms"][0]["marks"][0]["kind"], "circled_charge")
        doc["atoms"][0]["marks"][0]["offset"]["y"] = -15 * 42 / 14.4
        with self.assertRaisesRegex(ValueError, "distant atom marks"):
            handle(dict(protocol=1, operation="export", format="cdxml", document=doc))

    def test_builtin_catalog_matches_frozen_pubchem_formula_and_stereochemistry(self):
        catalog = json.loads((ROOT / "assets/template-catalog.json").read_text(encoding="utf-8"))
        library = json.loads((ROOT / "assets/templates.json").read_text(encoding="utf-8"))
        self.assertEqual(len(catalog), 79)
        self.assertEqual(len(library), len(catalog))
        self.assertEqual(sum(item["group"] == "Amino acids" for item in catalog), 20)
        for expected, item in zip(catalog, library):
            with self.subTest(name=expected["name"]):
                self.assertEqual(item["name"], expected["name"])
                for operation in ("analyze", "clean"):
                    result = handle(
                        dict(protocol=1, operation=operation, document=item["document"])
                    )
                    self.assertEqual(result["analysis"]["formula"], expected["formula"])
                    self.assertEqual(result["analysis"]["inchikey"], expected["inchikey"])
                if expected.get("sequence"):
                    mol = handle(
                        dict(
                            protocol=1, operation="export", format="mol", document=item["document"]
                        )
                    )["output"]
                    result = handle(dict(protocol=1, operation="import", format="mol", text=mol))
                    self.assertEqual(result["analysis"]["inchikey"], expected["inchikey"])

    def test_chemdraw_saved_ring_tools_are_molecules_and_keep_projected_positions(self):
        result = handle(
            dict(
                protocol=1,
                operation="import",
                format="cdxml",
                text=(ROOT / "tests/fixtures/ring-presets-chemdraw.cdxml").read_text(
                    encoding="utf-8"
                ),
            )
        )
        doc = result["document"]
        self.assertEqual(len(doc["atoms"]), 17)
        self.assertEqual(len(doc["bonds"]), 17)
        self.assertEqual(result["analysis"]["formula"], "C17H30")
        self.assertEqual(result["analysis"]["rings"], 3)
        self.assertEqual(sum(b["order"] == 2 for b in doc["bonds"]), 2)
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))["output"]
        restored = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertEqual(restored["analysis"]["smiles"], result["analysis"]["smiles"])
        for a, b in zip(doc["atoms"], restored["document"]["atoms"]):
            for axis in ("x", "y"):
                self.assertAlmostEqual(
                    a["position"][axis] - doc["atoms"][0]["position"][axis],
                    b["position"][axis] - restored["document"]["atoms"][0]["position"][axis],
                    places=3,
                )

    def test_chemdraw_saved_arrows_keep_head_styles_and_exact_bezier_controls(self):
        xml = (ROOT / "tests/fixtures/arrows-chemdraw.cdxml").read_text(encoding="utf-8")
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        arrows = result["document"]["arrows"]
        self.assertEqual(len(arrows), 7)
        self.assertEqual(len(result["document"]["graphics"]), 0)
        self.assertEqual([a["kind"] for a in arrows[:2]], ["curved", "fishhook"])
        self.assertEqual(arrows[1]["style"]["head"], "left")
        equilibrium = next(a for a in arrows if a["kind"] == "equilibrium")
        self.assertEqual(equilibrium["style"]["gap_pt"], 2.0)
        self.assertEqual(equilibrium["style"]["shape"], "open")
        self.assertTrue(any(a["style"]["dipole"] for a in arrows))
        self.assertTrue(any(a["style"]["no_go"] == "cross" for a in arrows))
        output = handle(
            dict(protocol=1, operation="export", format="cdxml", document=result["document"])
        )["output"]
        again = handle(dict(protocol=1, operation="import", format="cdxml", text=output))[
            "document"
        ]
        self.assertEqual([a["style"] for a in again["arrows"]], [a["style"] for a in arrows])
        for a, b in zip(arrows, again["arrows"]):
            for axis in ("x", "y"):
                self.assertAlmostEqual(
                    a["end"][axis] - a["start"][axis], b["end"][axis] - b["start"][axis], places=3
                )
                if "control" in a:
                    self.assertAlmostEqual(
                        a["control"][axis] - a["start"][axis],
                        b["control"][axis] - b["start"][axis],
                        places=3,
                    )

    def test_cdxml_arrow_exchange_rejects_combinations_the_reference_editor_changes(self):
        from engine.arrows_exchange import appearance

        doc = imported("CCO")["document"]
        a = dict(id=4, start={"x": 0, "y": 0}, end={"x": 180, "y": 0}, kind="forward")
        doc["arrows"] = [a]
        for kind, change in [
            ("forward", dict(pattern="dotted")),
            ("retro", {}),
            ("equilibrium", dict(equilibrium_ratio=0.5)),
            ("curved", dict(shape="hollow")),
            ("curved", dict(dipole=True)),
            ("dipole", dict(tail="full")),
        ]:
            with self.subTest(kind=kind, change=change):
                a["kind"] = kind
                a["style"] = {**appearance(dict(kind=kind)), **change}
                with self.assertRaises(ValueError):
                    handle(dict(protocol=1, operation="export", format="cdxml", document=doc))
        xml = (ROOT / "tests/fixtures/arrows-chemdraw.cdxml").read_text(encoding="utf-8")
        root = ET.fromstring(xml)
        root.find(".//arrow").set("AngularSize", "90")
        with self.assertRaisesRegex(ValueError, "elliptical"):
            handle(
                dict(
                    protocol=1,
                    operation="import",
                    format="cdxml",
                    text=ET.tostring(root, encoding="unicode"),
                )
            )

    def test_arrow_notches_colors_and_groups_survive_exchange_and_clean(self):
        from engine.arrows_exchange import appearance

        doc = imported("CCO")["document"]
        arrow = dict(id=4, start={"x": 0, "y": 100}, end={"x": 180, "y": 100}, kind="resonance")
        arrow["style"] = {
            **appearance(arrow),
            "shape": "hollow",
            "head_notch": 0.25,
            "color": [32, 80, 145],
            "head_length_pt": 4.0,
            "head_width_pt": 2.0,
        }
        doc["arrows"] = [arrow]
        doc["annotations"] = [dict(id=5, position={"x": 20, "y": 60}, text="conditions")]
        doc["groups"] = [dict(id=6, members=[4, 5])]
        clean = handle(dict(protocol=1, operation="clean", document=doc))["document"]
        self.assertEqual(clean["arrows"], doc["arrows"])
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))["output"]
        restored = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))[
            "document"
        ]
        self.assertEqual(restored["arrows"][0]["style"], arrow["style"])
        self.assertEqual(len(restored["groups"]), 1)
        self.assertEqual(
            set(restored["groups"][0]["members"]),
            {restored["arrows"][0]["id"], restored["annotations"][0]["id"]},
        )

    def test_chemdraw_saved_extended_bond_gallery_preserves_chemistry_and_appearance(self):
        xml = (ROOT / "tests/fixtures/bond-styles-chemdraw.cdxml").read_text(encoding="utf-8")
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        doc = result["document"]
        self.assertEqual(len(doc["bonds"]), 18)
        self.assertEqual(
            [(b["order"], b["display"]) for b in doc["bonds"]],
            [
                (1, "plain"),
                (2, "plain"),
                (3, "plain"),
                (1, "wedge"),
                (1, "hash"),
                (1, "hollow_wedge"),
                (1, "wavy"),
                (1, "bold"),
                (5, "dashed"),
                (0, "dotted"),
                (1, "plain"),
                (1, "hashed"),
                (7, "plain"),
                (7, "dashed"),
                (2, "bold"),
                (2, "wavy"),
                (5, "plain"),
                (6, "plain"),
            ],
        )
        hbond = next(b for b in doc["bonds"] if b["order"] == 0)
        h = next(a for a in doc["atoms"] if a["id"] == hbond["a"])
        acceptor = next(a for a in doc["atoms"] if a["id"] == hbond["b"])
        self.assertEqual(h["element"], "H")
        self.assertEqual(acceptor["label_h"], 3)
        self.assertEqual(doc["bonds"][12]["secondary_display"], "dashed")
        self.assertEqual(doc["bonds"][14]["secondary_display"], "plain")
        self.assertEqual(result["analysis"]["formula"], "C26H75Cu2N3ORe2+4")

    def test_bond_stereo_depictions_keep_identity_after_cleanup(self):
        for smiles in ["N[C@@H](C)C(=O)O", "F[C@](Cl)(Br)I", "F[C@@](Cl)(Br)I"]:
            for up_style in ("hollow_wedge", "bold"):
                with self.subTest(smiles=smiles, up_style=up_style):
                    self.check_stereo_depiction(smiles, up_style)

    def check_stereo_depiction(self, smiles, up_style):
        initial = imported(smiles)
        doc = initial["document"]
        for bond in doc["bonds"]:
            bond["display"] = {"wedge": up_style, "hash": "hashed"}.get(
                bond["display"], bond["display"]
            )
            bond["color"] = [32, 80, 145]
        for operation in ("analyze", "clean"):
            result = handle(dict(protocol=1, operation=operation, document=doc))
            self.assertEqual(result["analysis"]["smiles"], initial["analysis"]["smiles"])
            drawn = copy.deepcopy(result["document"])
            for atom in drawn["atoms"]:
                atom["stereo"] = None
            # Check the visible wedges, independently of stored absolute stereo.
            inferred = handle(dict(protocol=1, operation="analyze", document=drawn))
            self.assertEqual(inferred["analysis"]["smiles"], initial["analysis"]["smiles"])
            self.assertTrue(all(b["color"] == [32, 80, 145] for b in drawn["bonds"]))
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))["output"]
        self.assertEqual(
            handle(dict(protocol=1, operation="import", format="cdxml", text=xml))["analysis"][
                "smiles"
            ],
            initial["analysis"]["smiles"],
        )

    def test_extended_bond_orders_keep_valence_and_reject_lossy_molecular_exports(self):
        coord = imported("N->[Cu+2]")["document"]
        self.assertEqual(coord["bonds"][0]["order"], 5)
        coord["bonds"][0]["display"] = "dashed"
        r = handle(dict(protocol=1, operation="analyze", document=coord))
        self.assertEqual(r["analysis"]["formula"], "H3CuN+2")
        self.assertEqual(
            next(a for a in r["document"]["atoms"] if a["element"] == "N")["label_h"], 3
        )
        mol = handle(dict(protocol=1, operation="export", format="mol", document=coord))["output"]
        mol_roundtrip = handle(dict(protocol=1, operation="import", format="mol", text=mol))
        self.assertEqual(mol_roundtrip["document"]["bonds"][0]["order"], 5)
        self.assertEqual(mol_roundtrip["analysis"]["formula"], "H3CuN+2")
        quad = imported("[Re]$[Re]")["document"]
        self.assertEqual(quad["bonds"][0]["order"], 6)
        with self.assertRaisesRegex(ValueError, "cannot preserve"):
            handle(dict(protocol=1, operation="export", format="mol", document=quad))
        water = imported("O.N")["document"]
        oxygen = next(a for a in water["atoms"] if a["element"] == "O")
        nitrogen = next(a for a in water["atoms"] if a["element"] == "N")
        h = copy.deepcopy(oxygen)
        h.update(
            id=3,
            element="H",
            label_h=0,
            position={"x": oxygen["position"]["x"] + 42, "y": oxygen["position"]["y"]},
        )
        water["atoms"].append(h)
        water["bonds"] = [
            dict(a=oxygen["id"], b=3, order=1, display="plain"),
            dict(a=3, b=nitrogen["id"], order=0, display="dotted"),
        ]
        r = handle(dict(protocol=1, operation="analyze", document=water))
        self.assertEqual(r["analysis"]["formula"], "H5NO")
        self.assertEqual(r["analysis"]["smiles"], "")
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=water))["output"]
        reopened = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertEqual(reopened["analysis"]["formula"], "H5NO")
        self.assertEqual(sorted(b["order"] for b in reopened["document"]["bonds"]), [0, 1])
        for fmt in ["smiles", "mol", "inchi"]:
            with self.assertRaisesRegex(ValueError, "cannot preserve"):
                handle(dict(protocol=1, operation="export", format=fmt, document=water))
        water["bonds"][1]["a"] = oxygen["id"]
        with self.assertRaisesRegex(ValueError, "hydrogen bond"):
            handle(dict(protocol=1, operation="analyze", document=water))

    def test_cdxml_rejects_partial_parse_and_ambiguous_bond_style_mapping(self):
        bad = '<CDXML><page><fragment id="2"><n id="3" p="0 0" Element="6"/><n id="4" p="0 0" Element="6"/><b B="3" E="4"/></fragment></page></CDXML>'
        with self.assertRaisesRegex(ValueError, "safely associate"):
            handle(dict(protocol=1, operation="import", format="cdxml", text=bad))

    def test_chemdraw_saved_groups_keep_captions_attached_to_their_molecules(self):
        xml = (ROOT / "tests/fixtures/grouped-aspirin-chemdraw.cdxml").read_text(encoding="utf-8")
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        doc = result["document"]
        self.assertEqual(result["analysis"]["formula"], "C18H16O8")
        self.assertEqual(len(doc["groups"]), 2)
        for group in doc["groups"]:
            self.assertEqual(len(group["members"]), 14)
            labels = [a for a in doc["annotations"] if a["id"] in group["members"]]
            self.assertEqual([a["text"] for a in labels], ["Aspirin"])
            atoms = [a for a in doc["atoms"] if a["id"] in group["members"]]
            self.assertEqual(len(atoms), 13)
            self.assertTrue(
                min(a["position"]["x"] for a in atoms)
                < labels[0]["position"]["x"]
                < max(a["position"]["x"] for a in atoms)
            )

    def test_chemdraw_saved_graphics_keep_fill_stroke_and_nested_fragment_curves(self):
        xml = (ROOT / "tests/fixtures/graphics-chemdraw.cdxml").read_text(encoding="utf-8")
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertEqual(result["analysis"]["smiles"], "CC(=O)Oc1ccccc1C(=O)O")
        graphics = result["document"]["graphics"]
        # ChemDraw saves the two bracket strokes separately, one in the molecule.
        self.assertEqual(len(graphics), 5)
        self.assertEqual(sum(g["style"]["fill"] is not None for g in graphics), 2)
        self.assertEqual(graphics[0]["style"]["stroke"], [0, 0, 0])
        self.assertGreater(graphics[0]["style"]["fill"][1], 230)
        self.assertLess(graphics[0]["layer"], 0)
        self.assertTrue(all(g["kind"] == "path" for g in graphics))
        self.assertEqual(len({g["id"] for g in graphics}), 5)

    def test_supported_legacy_graphics_import_without_a_molecule(self):
        xml = """<CDXML BondLength="14.4"><page>
          <graphic GraphicType="Rectangle" BoundingBox="10 20 50 60" RectangleType="RoundEdge Filled Dashed"/>
          <graphic GraphicType="Bracket" BoundingBox="60 20 80 60" BracketType="SquarePair"/>
          <graphic GraphicType="Line" BoundingBox="10 80 80 90" LineType="Bold"/>
        </page></CDXML>"""
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertIsNone(result["analysis"])
        self.assertEqual(
            [g["kind"] for g in result["document"]["graphics"]],
            ["rounded_rectangle", "brackets", "line"],
        )
        self.assertEqual(result["document"]["graphics"][0]["style"]["pattern"], "dashed")

    def test_unsupported_graphic_styles_and_group_objects_fail_explicitly(self):
        points = "0 0 0 0 0 0 10 10 10 10 10 10"
        for content in [
            '<graphic GraphicType="Orbital" BoundingBox="0 0 10 10"/>',
            f'<curve CurvePoints="{points}" LineType="Wavy"/>',
            f'<curve CurvePoints="{points}" CurveType="512"/>',
            f'<curve CurvePoints="{points}" alpha="0.5"/>',
            f'<group><curve CurvePoints="{points}"/><spectrum/></group>',
        ]:
            with self.subTest(content=content), self.assertRaises(ValueError):
                handle(
                    dict(
                        protocol=1,
                        operation="import",
                        format="cdxml",
                        text=f"<CDXML><page>{content}</page></CDXML>",
                    )
                )
        doc = {
            "version": 4,
            "atoms": [],
            "bonds": [],
            "graphics": [{"id": 1, "style": {"pattern": "dotted"}}],
        }
        path = [
            {"command": "move", "points": {"x": 0, "y": 0}},
            {"command": "line", "points": {"x": 10, "y": 10}},
        ]
        with self.assertRaisesRegex(ValueError, "Dotted graphics"):
            handle(
                dict(
                    protocol=1,
                    operation="export",
                    format="cdxml",
                    document=doc,
                    graphic_paths={"1": path},
                )
            )

    def test_molecule_identity_survives_editable_graph_and_cleanup(self):
        for smiles in [
            "CCO",
            "c1ccccc1",
            "CC(=O)Oc1ccccc1C(=O)O",
            "[Na+].[Cl-]",
            "[13CH3]O",
            "N[C@@H](C)C(=O)O",
            "F[C@](Cl)(Br)I",
            "F/C=C/F",
            "F/C=C\\F",
        ]:
            with self.subTest(smiles=smiles):
                initial = imported(smiles)
                doc = initial["document"]
                for op in ["analyze", "clean"]:
                    result = handle(dict(protocol=1, operation=op, document=doc))
                    self.assertEqual(initial["analysis"]["smiles"], result["analysis"]["smiles"])
                    self.assertEqual(
                        [a["id"] for a in doc["atoms"]],
                        [a["id"] for a in result["document"]["atoms"]],
                    )

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
        doc["annotations"] = [{"id": 100, "position": {"x": 5, "y": 8}, "text": "heat"}]
        doc["arrows"] = [{"id": 101, "start": {"x": 0, "y": 0}, "end": {"x": 42, "y": 0}}]
        result = handle(dict(protocol=1, operation="clean", document=doc))["document"]
        self.assertEqual(doc["annotations"], result["annotations"])
        self.assertEqual(doc["arrows"], result["arrows"])
        for axis in ["x", "y"]:
            self.assertAlmostEqual(
                sum(a["position"][axis] for a in doc["atoms"]),
                sum(a["position"][axis] for a in result["atoms"]),
            )

    def test_invalid_valence_is_rejected(self):
        with self.assertRaises(ValueError):
            imported("C(C)(C)(C)(C)C")

    def test_mol_and_cdxml_round_trips(self):
        for smiles in ["CCO", "c1ccccc1", "N[C@@H](C)C(=O)O", "F/C=C/F", "[13CH3]O"]:
            initial = imported(smiles)
            for fmt in ["mol", "cdxml"]:
                with self.subTest(smiles=smiles, format=fmt):
                    output = handle(
                        dict(
                            protocol=1, operation="export", format=fmt, document=initial["document"]
                        )
                    )["output"]
                    result = handle(dict(protocol=1, operation="import", format=fmt, text=output))
                    self.assertEqual(initial["analysis"]["smiles"], result["analysis"]["smiles"])

    def test_cdxml_preserves_supported_annotations(self):
        doc = imported("CCO")["document"]
        doc["annotations"] = [{"id": 50, "position": {"x": 0, "y": 0}, "text": "heat"}]
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))["output"]
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertEqual(result["document"]["annotations"][0]["text"], "heat")

    def test_rich_text_survives_cleanup_and_cdxml_exchange(self):
        doc = imported("CCO")["document"]
        style = {
            **TEXT_DEFAULTS,
            "family": "Helvetica",
            "size_pt": 12.0,
            "italic": True,
            "formula": True,
        }
        emphasis = {**style, "bold": True, "underline": True, "color": [32, 80, 145]}
        # The emphasis starts after a multibyte Greek character.
        text = "α Pd/C, H2\nEtOH · 25 °C"
        doc["annotations"] = [
            {
                "id": 50,
                "position": {"x": 0, "y": 100},
                "text": text,
                "format": {
                    "style": style,
                    "spans": [{"start": 3, "end": 7, "style": emphasis}],
                    "alignment": "center",
                    "line_spacing": 1.5,
                    "width_pt": 120,
                },
            }
        ]
        doc["atoms"][-1]["text_style"] = emphasis
        for op in ("analyze", "clean"):
            result = handle(dict(protocol=1, operation=op, document=doc))["document"]
            self.assertEqual(result["annotations"], doc["annotations"])
            self.assertEqual(result["atoms"][-1]["text_style"], emphasis)
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))["output"]
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        label = result["document"]["annotations"][0]
        self.assertEqual(result["analysis"]["smiles"], "CCO")
        self.assertEqual(label["text"], text)
        self.assertEqual(label["format"], doc["annotations"][0]["format"])
        self.assertEqual(
            result["document"]["atoms"][-1]["text_style"], {**emphasis, "formula": False}
        )
        runs = list(text_runs(label["text"], label["format"]))
        self.assertEqual(runs[1], ("Pd/C", emphasis))

    def test_cdxml_text_without_molecule_and_explicit_scripts(self):
        style = {**TEXT_DEFAULTS, "script": "superscript", "color": [180, 50, 55]}
        doc = {
            "version": 3,
            "atoms": [],
            "bonds": [],
            "arrows": [],
            "annotations": [
                {
                    "id": 1,
                    "position": {"x": 0, "y": 0},
                    "text": "Fe3+",
                    "format": {
                        "style": TEXT_DEFAULTS,
                        "spans": [{"start": 2, "end": 4, "style": style}],
                    },
                }
            ],
        }
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))["output"]
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertIsNone(result["analysis"])
        label = result["document"]["annotations"][0]
        self.assertEqual(label["text"], "Fe3+")
        self.assertEqual(label["format"]["spans"], doc["annotations"][0]["format"]["spans"])

    def test_atom_typography_preserves_charge_isotope_and_skeletal_carbons(self):
        for smiles in ("CCO", "[13CH3]O", "[NH4+]", "[O-]C=O"):
            doc = imported(smiles)["document"]
            for a in doc["atoms"]:
                a["text_style"] = {**TEXT_DEFAULTS, "bold": True, "size_pt": 14.0}
            xml = handle(dict(protocol=1, operation="export", format="cdxml", document=doc))[
                "output"
            ]
            result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
            self.assertEqual(result["analysis"]["smiles"], imported(smiles)["analysis"]["smiles"])
            self.assertTrue(all(a["text_style"]["bold"] for a in result["document"]["atoms"]))
            if smiles == "CCO":
                self.assertEqual(len(ET.fromstring(xml).findall(".//n/t")), 1)

    def test_cdxml_rejects_text_styles_it_cannot_preserve(self):
        xml = '<CDXML><page><t p="0 0"><s face="8">Outlined</s></t></page></CDXML>'
        with self.assertRaisesRegex(ValueError, "Outlined/shadowed"):
            handle(dict(protocol=1, operation="import", format="cdxml", text=xml))

    def test_chemdraw_saved_typography_uses_reserved_colors_and_bounding_box(self):
        xml = (ROOT / "tests/fixtures/formatted-label-chemdraw.cdxml").read_text(encoding="utf-8")
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        label = result["document"]["annotations"][0]
        self.assertEqual(label["text"], "Pd/C, H2\nEtOH · 25 °C")
        runs = list(text_runs(label["text"], label["format"]))
        self.assertTrue(runs[0][1]["bold"])
        self.assertGreater(runs[0][1]["color"][2], 140)
        self.assertEqual(runs[1][1]["color"], [0, 0, 0])
        self.assertFalse(runs[1][1]["bold"])
        self.assertTrue(
            all(
                a.get("text_style", TEXT_DEFAULTS)["color"] == [0, 0, 0]
                for a in result["document"]["atoms"]
            )
        )
        bbox = ET.fromstring(xml).find("./page/t").get("BoundingBox").split()
        self.assertAlmostEqual(label["position"]["x"], float(bbox[0]) * 42 / 14.4)

    def test_reference_ui_export_imports_as_ethanol(self):
        xml = (ROOT / "tests/fixtures/reference-ethanol.cdxml").read_text(encoding="utf-8")
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
        self.assertAlmostEqual(
            sum((a - b) ** 2 for a, b in zip(first, second)) ** 0.5, 14.4, places=4
        )
        result = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))
        self.assertEqual(result["analysis"]["smiles"], "CCO")
        for before, after in zip(doc["atoms"], result["document"]["atoms"]):
            # Import may translate, but must preserve relative geometry and scale.
            for axis in ("x", "y"):
                self.assertAlmostEqual(
                    before["position"][axis] - doc["atoms"][0]["position"][axis],
                    after["position"][axis] - result["document"]["atoms"][0]["position"][axis],
                    places=4,
                )

    def test_freehand_ui_saved_drawing_and_exports(self):
        fixtures = ROOT / "tests/fixtures"
        doc = json.loads((fixtures / "ui-drawn-ethanol.moruno").read_text(encoding="utf-8"))
        self.assertEqual(Chem.MolToSmiles(from_document(doc)), "CCO")
        self.assertEqual(doc["annotations"][0]["text"], "oxidation")
        self.assertEqual(len(doc["arrows"]), 1)
        xml = (fixtures / "ui-drawn-ethanol.cdxml").read_text(encoding="utf-8")
        self.assertEqual(Chem.MolToSmiles(Chem.MolsFromCDXML(xml)[0]), "CCO")
        self.assertEqual(len(ET.fromstring(xml).findall(".//arrow")), 1)
        roundtrip = handle(dict(protocol=1, operation="import", format="cdxml", text=xml))[
            "document"
        ]
        self.assertEqual(roundtrip["annotations"][0]["text"], "oxidation")
        self.assertEqual(roundtrip["arrows"][0]["kind"], "forward")
        original_delta = doc["arrows"][0]["start"]["x"] - doc["atoms"][0]["position"]["x"]
        imported_delta = (
            roundtrip["arrows"][0]["start"]["x"] - roundtrip["atoms"][0]["position"]["x"]
        )
        self.assertAlmostEqual(original_delta, imported_delta, places=3)
        svg = ET.parse(fixtures / "ui-drawn-ethanol.svg")
        self.assertIn("oxidation", "".join(svg.getroot().itertext()))

    def test_worker_recovers_after_bad_request_and_keeps_ids(self):
        requests = [
            "not json",
            "[]",
            json.dumps(dict(id=2, protocol=1, operation="import", format="smiles", text="CCO")),
        ]
        proc = subprocess.run(
            [sys.executable, str(ROOT / "engine/worker.py")],
            input="\n".join(requests) + "\n",
            capture_output=True,
            text=True,
            timeout=15,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        responses = [json.loads(line) for line in proc.stdout.splitlines()]
        self.assertFalse(responses[0]["ok"])
        self.assertFalse(responses[1]["ok"])
        self.assertTrue(responses[2]["ok"])
        self.assertEqual(responses[2]["id"], 2)
        self.assertEqual(responses[2]["result"]["analysis"]["formula"], "C2H6O")


if __name__ == "__main__":
    unittest.main()
