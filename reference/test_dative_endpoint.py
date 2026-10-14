"""Independent source/evidence bounds for the shared End-paint adapter."""

import contextlib
import io
import itertools
import unittest
from unittest.mock import patch

import cdxml_preparation_reference as preparation
from cdxml_molecular_reference import xml
from rdkit import RDLogger
from reference_dative_endpoint import bounded_source, evidence


class DativeEndpointEvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        RDLogger.DisableLog("rdApp.*")

    def test_exactly_twelve_corpus_sources_have_pinned_old_and_new_evidence(self):
        sources = {}
        with patch.object(
            preparation,
            "emit",
            side_effect=lambda name, text, *a, **k: sources.update({name: text}),
        ):
            with contextlib.redirect_stdout(io.StringIO()):
                preparation.main()
        qualified = {
            name
            for name, text in sources.items()
            if isinstance(text, str) and bounded_source(text) is not None
        }
        expected = {
            f"bond/dative/{display}/{secondary}"
            for display, secondary in itertools.product(
                ("WedgeEnd", "WedgedHashEnd", "HollowWedgeEnd"), (None, "Solid", "Dash", "Bold")
            )
        }
        self.assertEqual(qualified, expected)
        for name in sorted(expected):
            with self.subTest(name=name):
                derived = evidence(sources[name])
                self.assertIsNotNone(derived)
                self.assertEqual(derived["labels"], name.endswith("/None"))
                self.assertEqual(derived["endpoints"], [0, 1])

    def test_unproved_sources_receive_no_evidence(self):
        attrs = dict(Order="dative", Display="WedgeEnd")
        original = xml([{}, {}], [(0, 1, attrs)])
        controls = [
            xml([{}, {}], [(0, 1, {**attrs, "Display": "WedgeBegin"})]),
            xml([{}, {}], [(0, 1, {**attrs, "Order": "1"})]),
            xml([{}, {}], [(0, 1, {**attrs, "Display2": "DottedHydrogen"})]),
            xml([dict(Element="7"), {}], [(0, 1, attrs)]),
            xml([dict(Element="6"), {}], [(0, 1, attrs)]),
            xml([dict(Charge="0"), {}], [(0, 1, attrs)]),
            xml([dict(NumHydrogens="0"), {}], [(0, 1, attrs)]),
            xml([{}, {}, {}], [(0, 1, attrs)]),
            xml([{}, {}, {}], [(0, 1, attrs), (1, 2, {})]),
            original.replace("</page>", "<fragment id='8'/></page>"),
            original.replace("</page>", "<n id='9'/></page>"),
            original.replace("</fragment>", "<t>label</t></fragment>"),
            original.replace('Display="WedgeEnd"', 'Display="WedgeEnd" BS="E"'),
            original.replace('BondLength="14.4"', 'BondLength="28.8"'),
        ]
        for text in controls:
            with self.subTest(text=text):
                self.assertIsNone(evidence(text))


if __name__ == "__main__":
    unittest.main()
