"""Independent CDXML paint and directed-Line expectations, without RDKit."""

import unittest
import xml.etree.ElementTree as ET

from engine.graphics_exchange import read_graphics, write_graphics

TRIANGLE = [
    {"command": "move", "points": {"x": 1, "y": 2}},
    {"command": "line", "points": {"x": 5, "y": 2}},
    {"command": "line", "points": {"x": 3, "y": 6}},
    {"command": "close"},
]
TRIANGLE_POINTS = "1 2 1 2 1 2 5 2 5 2 5 2 3 6 3 6 3 6"
OPEN_PATH = TRIANGLE[:2]


def exported(graphic, *, parts=None, paths=None):
    page = ET.Element("page")
    result = write_graphics(
        page,
        [graphic],
        paths,
        1,
        lambda p: f"{p['x']} {p['y']}",
        lambda rgb: ",".join(map(str, rgb)),
        10,
        parts=parts,
    )
    return page, result


def part(style, commands=TRIANGLE):
    return {"commands": commands, "style": style}


class GraphicPaintExportTests(unittest.TestCase):
    def test_fill_only_background_has_no_zero_width_outline(self):
        # A condition icon consists of a paper disc, then a separately stroked
        # glyph. A zero-width CDXML stroke is still a stroke, not an absent one.
        page, (next_id, middle, objects) = exported(
            {"id": 7, "kind": {"symbol": "light"}},
            parts={
                "7": [
                    part({"fill": [255, 255, 255], "width_pt": 0}),
                    part({"stroke": [12, 34, 56], "width_pt": 0.75}, OPEN_PATH),
                ]
            },
        )
        self.assertEqual((next_id, middle), (14, 2))
        self.assertIs(objects[7], page[0])
        curves = page.findall(".//curve")
        self.assertEqual(len(curves), 2)
        self.assertEqual(
            curves[0].attrib,
            {
                "id": "12",
                "Z": "1",
                "CurvePoints": TRIANGLE_POINTS,
                "CurveType": "129",
                "Closed": "yes",
                "FillType": "Solid",
                "LineType": "Solid",
                "LineWidth": "0",
                "color": "255,255,255",
            },
        )
        self.assertEqual(curves[1].get("FillType"), "None")
        self.assertEqual(curves[1].get("LineWidth"), "0.75")
        self.assertEqual(curves[1].get("color"), "12,34,56")

    def test_closed_shading_is_fill_only_and_keeps_exact_geometry(self):
        page, _ = exported(
            {"id": 7, "kind": {"orbital": "p"}},
            parts={
                "7": [
                    part(
                        {
                            "fill": [40, 80, 120],
                            "stroke": [5, 10, 15],
                            "pattern": "dashed",
                            "width_pt": 0.6,
                            "effects": {"shaded": True},
                        }
                    )
                ]
            },
        )
        fill, outline = page.findall(".//curve")
        self.assertEqual(fill.get("CurveType"), "257")
        self.assertEqual(fill.get("FillType"), "Shaded")
        self.assertEqual(fill.get("LineType"), "Solid")
        self.assertEqual(fill.get("LineWidth"), "0")
        self.assertEqual(fill.get("color"), "40,80,120")
        self.assertEqual(outline.get("CurveType"), "3")
        self.assertEqual(outline.get("FillType"), "None")
        self.assertEqual(outline.get("LineType"), "Dashed")
        self.assertEqual(outline.get("color"), "5,10,15")
        for curve in (fill, outline):
            self.assertEqual(curve.get("CurvePoints"), TRIANGLE_POINTS)

    def test_open_fill_does_not_acquire_closed_shading(self):
        page, _ = exported(
            {"id": 7, "kind": "path"},
            parts={
                "7": [
                    part(
                        {
                            "fill": [40, 80, 120],
                            "width_pt": 0,
                            "effects": {"shaded": True},
                        },
                        OPEN_PATH,
                    )
                ]
            },
        )
        curves = page.findall(".//curve")
        self.assertEqual(len(curves), 1)
        self.assertEqual(curves[0].get("CurveType"), "128")
        self.assertEqual(curves[0].get("FillType"), "Solid")
        self.assertEqual(curves[0].get("Closed"), "no")

    def test_fill_only_parts_still_reject_invalid_geometry_and_dots(self):
        for commands, style, message in [
            (TRIANGLE[1:], {"fill": [0, 0, 0], "width_pt": 0}, "start at an anchor"),
            (TRIANGLE[:1], {"fill": [0, 0, 0], "width_pt": 0}, "at least two anchors"),
            (TRIANGLE, {"fill": [0, 0, 0], "width_pt": 0, "pattern": "dotted"}, "Dotted graphics"),
        ]:
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                exported({"id": 7, "kind": "path"}, parts={"7": [part(style, commands)]})


class NativeLineExportTests(unittest.TestCase):
    def line(self, **overrides):
        return {
            "id": 7,
            "kind": "line",
            "layer": 0,
            "origin": {"x": 9, "y": -2},
            "axis_x": {"x": -6, "y": 10},
            "axis_y": {"x": 100, "y": 200},
            **overrides,
        }

    def test_native_line_uses_directed_frame_and_stroke_for_each_pattern(self):
        for pattern in ("solid", "dashed", "wavy"):
            for heads in (None, {}, {"head": "none", "tail": "none"}):
                with self.subTest(pattern=pattern, heads=heads):
                    page, (next_id, middle, objects) = exported(
                        self.line(
                            heads=heads,
                            style={
                                "stroke": [12, 34, 56],
                                "fill": [90, 80, 70],
                                "pattern": pattern,
                                "width_pt": 1.25,
                            },
                        ),
                        # Native Lines use their authored frame even when a
                        # sampled path was also supplied for another format.
                        paths={"7": TRIANGLE},
                    )
                    self.assertEqual((next_id, middle), (11, 1))
                    self.assertEqual(len(page), 1)
                    self.assertIs(objects[7], page[0])
                    self.assertEqual(page[0].tag, "graphic")
                    self.assertEqual(
                        page[0].attrib,
                        {
                            "id": "10",
                            "Z": "2",
                            "GraphicType": "Line",
                            "LineType": pattern.capitalize(),
                            "BoundingBox": "9 -2 3 8",
                            "LineWidth": "1.25",
                            "color": "12,34,56",
                        },
                    )

    def test_plain_line_needs_no_sampled_geometry(self):
        page, _ = exported(self.line())
        self.assertEqual(page[0].get("BoundingBox"), "9 -2 3 8")
        self.assertEqual(page[0].get("LineWidth"), "0.6")
        self.assertEqual(page[0].get("color"), "0,0,0")

    def test_headed_lines_and_non_lines_keep_the_curve_route(self):
        for changes in (
            {"heads": {"head": "full", "tail": "none"}},
            {"heads": {"head": "none", "tail": "full"}},
            {"kind": "path"},
            {"kind": "curve"},
            {"style": {"effects": {"shaded": True}}},
        ):
            with self.subTest(changes=changes):
                page, _ = exported(self.line(**changes), paths={"7": OPEN_PATH})
                self.assertEqual(page[0].tag, "curve")
                self.assertEqual(page[0].get("CurveType"), "0")
                self.assertEqual(page[0].get("FillType"), "None")

    def test_dotted_line_keeps_explicit_unsupported_error(self):
        with self.assertRaisesRegex(ValueError, "Dotted graphics"):
            exported(self.line(style={"pattern": "dotted"}), paths={"7": OPEN_PATH})


class GraphicDiscoveryTests(unittest.TestCase):
    def test_container_claims_do_not_claim_descendant_graphics(self):
        for tag in ("fragment", "table", "page"):
            for claim in (None, "container", "graphic"):
                with self.subTest(tag=tag, claim=claim):
                    root = ET.fromstring(
                        f'<CDXML><page><{tag} id="same"><graphic id="same" '
                        f'GraphicType="Rectangle" BoundingBox="1 2 3 4"/></{tag}></page></CDXML>'
                    )
                    container = root[0][0]
                    graphic = container[0]
                    claimed = {"container": container, "graphic": graphic}.get(claim)
                    objects = {} if claimed is None else {claimed: [99]}
                    before = ET.tostring(root)
                    result = read_graphics(root, 1, 100, objects)
                    self.assertEqual(ET.tostring(root), before)
                    if claimed is not None:
                        self.assertEqual(objects[claimed], [99])
                    if claim == "graphic":
                        self.assertEqual(result, [])
                    else:
                        self.assertEqual(len(result), 1)
                        self.assertEqual(result[0]["id"], 100)
                        self.assertEqual(result[0]["origin"], {"x": 1, "y": 2})
                        self.assertEqual(result[0]["axis_x"], {"x": 2, "y": 0})
                        self.assertEqual(result[0]["axis_y"], {"x": 0, "y": 2})
                        self.assertEqual(objects[graphic], [100])

    def test_table_cell_descent_preserves_leaf_identity_and_excludes_unknown_containers(self):
        root = ET.fromstring(
            '<CDXML><page><table><page><graphic GraphicType="Rectangle" '
            'BoundingBox="1 2 3 4"/></page></table><unknown><page>'
            '<graphic GraphicType="Rectangle" BoundingBox="5 6 7 8"/>'
            "</page></unknown></page></CDXML>"
        )
        table, unknown = root[0]
        cell = table[0]
        graphic = cell[0]
        objects = {table: [98], cell: [99]}
        result = read_graphics(root, 1, 100, objects)
        self.assertEqual([g["id"] for g in result], [100])
        self.assertEqual(objects, {table: [98], cell: [99], graphic: [100]})
        self.assertNotIn(unknown[0][0], objects)


if __name__ == "__main__":
    unittest.main()
