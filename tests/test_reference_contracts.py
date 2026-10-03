import unittest
import xml.etree.ElementTree as ET

from reference_annotations import legacy_preparation_input
from reference_presentation import compare


class ReferenceContractTests(unittest.TestCase):
    def test_rgb_bias_preserves_each_native_and_binary_color_channel(self):
        for channel in range(1, 255):
            before = f"{channel / 255:.8f}"
            after = f"{(channel * 100_000_000 // 255 + 1) / 100_000_000:.8f}"
            if before == after:
                continue
            for axis in ("r", "g", "b"):
                expected = f'<CDXML><colortable><color {axis}="{before}"/></colortable></CDXML>'
                actual = expected.replace(before, after)
                with self.subTest(channel=channel, axis=axis):
                    compare(actual, expected, "cdxml")

    def test_rgb_adjustment_rejects_other_colors_attributes_and_structural_changes(self):
        expected = '<CDXML><colortable><color r="0.27450980" g="0.07843137" b="0"/></colortable><page><n Element="8" p="1 2"/></page></CDXML>'
        actual = expected.replace("0.27450980", "0.27450981").replace("0.07843137", "0.07843138")
        compare(actual, expected, "cdxml")
        for changed in (
            actual.replace("0.27450981", "0.27843138"),
            actual.replace("0.27450981", "0.27450982"),
            actual.replace("0.27450981", "0.27450979"),
            actual.replace("0.27450981", "nan"),
            actual.replace("0.27450981", "inf"),
            actual.replace("0.27450981", "-0.1"),
            actual.replace("0.27450981", "1.1"),
            actual.replace(' b="0"', ""),
            actual.replace(' b="0"', ' b="0" color="3"'),
            actual.replace('Element="8"', 'Element="7"'),
            actual.replace('p="1 2"', 'p="1 3"'),
            actual.replace("<color ", "<other "),
        ):
            with self.subTest(actual=changed), self.assertRaises(ValueError):
                compare(changed, expected, "cdxml")
        with self.assertRaises(ValueError):
            compare('<CDXML r="0.27450981"/>', '<CDXML r="0.27450980"/>', "cdxml")

    def test_equivalent_f32_percentages_are_compared_without_losing_other_fields(self):
        expected = '<CDXML BondSpacing="11.999999731779099"><page><n Element="8"/></page></CDXML>'
        actual = expected.replace("11.999999731779099", "12")
        compare(actual, expected, "cdxml")
        for changed in (
            actual.replace('BondSpacing="12"', 'BondSpacing="11.9"'),
            actual.replace('Element="8"', 'Element="7"'),
            actual.replace('BondSpacing="12"', 'BondSpacing="nan"'),
        ):
            with self.assertRaises(ValueError):
                compare(changed, expected, "cdxml")

    def test_only_named_fixture_metadata_is_removed_for_legacy_preparation(self):
        name = "molecular-fixture/geometry-tetrahedral-4.cdxml"
        text = (
            '<CDXML><page><fragment><n id="1" Element="8" p="1 2"/></fragment>'
            '<annotation Keyword="Name" Content="example"/></page></CDXML>'
        )
        self.assertEqual(legacy_preparation_input("another fixture", text), text)
        adapted = ET.fromstring(legacy_preparation_input(name, text))
        self.assertEqual(list(adapted.iter("annotation")), [])
        self.assertEqual(next(adapted.iter("n")).attrib, {"id": "1", "Element": "8", "p": "1 2"})
        with self.assertRaises(AssertionError):
            legacy_preparation_input(name, text.replace('Keyword="Name"', 'Keyword="Other"'))
