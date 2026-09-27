import unittest

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
