"""Cubic geometry stays independently editable at the exchange boundary."""

import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from engine.arrows_exchange import control, read_arrow, write_arrow


def point(text):
    x, y, *_ = map(float, text.split())
    return dict(x=x, y=y)


def position(value):
    return f"{value['x']:.17g} {value['y']:.17g}"


class CubicControlsTest(unittest.TestCase):
    def source(self):
        fixture = (
            Path(__file__).resolve().parents[1]
            / "tests/fixtures/mechanism-curvature-91/independent-controls.cdxml"
        )
        root = ET.parse(fixture).getroot()
        return root, root.find(".//curve")

    def test_independent_controls_and_heads_round_trip_exactly(self):
        root, node = self.source()
        for head in ("Full", "HalfLeft", "HalfRight"):
            node.set("ArrowheadHead", head)
            arrow = read_arrow(node, root, point, [[0, 0, 0]] * 4, 1)
            self.assertEqual(arrow["cubic"], [dict(x=100, y=76), dict(x=136, y=124)])
            self.assertNotIn("control", arrow)
            self.assertIsNone(control(arrow))
            output = ET.Element("page")
            saved = write_arrow(output, arrow, 2, position, lambda _: "3")
            restored = read_arrow(saved, root, point, [[0, 0, 0]] * 4, 1)
            self.assertEqual(restored, arrow)

    def test_small_independent_control_difference_is_never_approximated(self):
        root, node = self.source()
        node.set("CurvePoints", "100 100 100 100 112 112 124 112.000001 136 100 136 100")
        arrow = read_arrow(node, root, point, [[0, 0, 0]] * 4, 1)
        self.assertEqual(arrow["cubic"][1], dict(x=124, y=112.000001))
        self.assertNotIn("control", arrow)

    def test_cubic_decorations_keep_explicit_lossy_exchange_errors(self):
        root, node = self.source()
        arrow = read_arrow(node, root, point, [[0, 0, 0]] * 4, 1)
        for property_name, value in (("pattern", "dotted"), ("shape", "hollow"), ("dipole", True)):
            changed = dict(arrow, style=dict(arrow["style"]))
            changed["style"][property_name] = value
            with self.assertRaisesRegex(ValueError, "native/SVG/PDF/PNG"):
                write_arrow(ET.Element("page"), changed, 2, position, lambda _: "3")


if __name__ == "__main__":
    unittest.main()
