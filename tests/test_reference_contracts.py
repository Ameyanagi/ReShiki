import unittest
import xml.etree.ElementTree as ET

from reference_annotations import legacy_preparation_input
from reference_presentation import compare


class ReferenceContractTests(unittest.TestCase):
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
