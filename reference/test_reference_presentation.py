"""The legacy comparison adjustment must not hide chemical or layout differences."""

import base64
import struct
import unittest
import xml.etree.ElementTree as ET
from copy import deepcopy
from unittest.mock import patch

from reference_presentation import compare

from engine.cdx_exchange import PROPERTIES, to_cdx


class ReferencePresentationTests(unittest.TestCase):
    legacy = '<CDXML><page id="1"><fragment id="2"><n id="3" p="0 0" Element="0"><t p="0 0" LabelAlignment="Auto"><s>*</s></t></n><n id="4" p="42 0" Element="6"/><b id="5" B="3" E="4" Order="1"/></fragment></page></CDXML>'

    def hidden(self):
        root = ET.fromstring(self.legacy)
        node = root.find(".//n")
        node.set("Visible", "no")
        node.set("NodeType", "Unspecified")
        node.set("NumHydrogens", "0")
        del node.find("t").attrib["LabelAlignment"]
        return ET.tostring(root, encoding="unicode")

    def test_accepts_only_documented_hidden_dummy_delta_in_both_formats(self):
        actual = self.hidden()
        compare(actual, self.legacy, "cdxml")
        compare(
            base64.b64encode(to_cdx(actual)).decode(),
            base64.b64encode(to_cdx(self.legacy)).decode(),
            "cdx",
        )

    def test_does_not_ignore_chemistry_geometry_labels_or_missing_handles(self):
        for old, new in [
            ('Element="6"', 'Element="7"'),
            ('Order="1"', 'Order="2"'),
            ('E="4"', 'E="3"'),
            ('p="42 0"', 'p="43 0"'),
            ('Visible="no"', 'Visible="yes"'),
            ('NumHydrogens="0"', 'NumHydrogens="1"'),
            ('NodeType="Unspecified"', 'NodeType="MultiAttachment"'),
            ('Element="0"', 'Element="0" Charge="1"'),
            ("<s>*</s>", "<s>M</s>"),
        ]:
            actual = self.hidden().replace(old, new)
            for format in ["cdxml", "cdx"]:
                if format == "cdx":
                    changed = base64.b64encode(to_cdx(actual)).decode()
                    legacy = base64.b64encode(to_cdx(self.legacy)).decode()
                else:
                    changed, legacy = actual, self.legacy
                with self.subTest(change=(old, new), format=format), self.assertRaises(ValueError):
                    compare(changed, legacy, format)
        with self.assertRaises(ValueError):
            compare(self.legacy, self.legacy, "cdxml")

    def test_reports_all_difference_categories_and_object_locations(self):
        actual = (
            self.hidden()
            .replace('Element="6"', 'Element="7" NumHydrogens="0"')
            .replace('p="42 0"', 'p="43 0"')
            .replace("<s>*</s>", "<s>M</s>")
        )
        with self.assertRaises(ValueError) as result:
            compare(actual, self.legacy, "cdxml")
        diagnostic = str(result.exception)
        for field in ("@Element", "@NumHydrogens", "@p", "[text]", "/n[1]"):
            self.assertIn(field, diagnostic)
        self.assertIn("attribute=3", diagnostic)
        self.assertIn("text=1", diagnostic)

    def test_reports_every_missing_or_extra_object_with_its_fields(self):
        extras = '<n id="6" Element="7" Charge="1"><t><s>EXTRA</s></t></n><n id="7" Element="8"/>'
        for added in (True, False):
            actual = self.hidden()
            expected = self.legacy
            if added:
                actual = actual.replace("</fragment>", extras + "</fragment>")
            else:
                expected = expected.replace("</fragment>", extras + "</fragment>")
            for format in ("cdxml", "cdx"):
                a, e = self.format_pair(actual, expected, format)
                with (
                    self.subTest(added=added, format=format),
                    self.assertRaises(ValueError) as result,
                ):
                    compare(a, e, format)
                diagnostic = str(result.exception)
                category = "object-added" if added else "object-missing"
                self.assertEqual(4, diagnostic.count(f"[{category}]"))
                self.assertIn(f"{category}=4", diagnostic)
                self.assertIn("'Charge': '1'", diagnostic)
                self.assertIn("text='EXTRA'", diagnostic)

    def zero_hydrogen_pair(self, element="N", charge=1):
        atomic_number = {"H": 1, "C": 6, "N": 7, "O": 8}[element]
        charge_attr = f' Charge="{charge}"' if charge else ""
        expected = f'<CDXML><page><fragment><n id="3" Element="{atomic_number}"{charge_attr} p="1 2"><t><s>{element}</s></t></n></fragment></page></CDXML>'
        actual = expected.replace(' id="3"', ' id="3" NumHydrogens="0"')
        # Persistent source IDs intentionally differ from the writer's index IDs.
        source = [
            dict(
                id=9007199254740993, element=element, charge=charge, explicit_h=0, no_implicit=True
            )
        ]
        return actual, expected, source

    def format_pair(self, actual, expected, format):
        if format == "cdx":
            return base64.b64encode(to_cdx(actual)).decode(), base64.b64encode(
                to_cdx(expected)
            ).decode()
        return actual, expected

    def compare_both_formats(self, actual, expected, source):
        compare(actual, expected, "cdxml", source)
        compare(
            base64.b64encode(to_cdx(actual)).decode(),
            base64.b64encode(to_cdx(expected)).decode(),
            "cdx",
            source,
        )

    def test_zero_hydrogen_addition_requires_exact_independent_source_predicate(self):
        for element, charge in (
            ("H", 0),
            ("O", 0),
            ("N", 0),
            ("N", 1),
            ("O", -1),
            ("C", 1),
            ("C", -1),
        ):
            with self.subTest(element=element, charge=charge):
                self.compare_both_formats(*self.zero_hydrogen_pair(element, charge))
        actual, expected, source = self.zero_hydrogen_pair()
        for changed in (
            None,
            [],
            [dict(source[0], no_implicit=False)],
            [dict(source[0], no_implicit=1)],
            [dict(source[0], explicit_h=1)],
            [dict(source[0], explicit_h=False)],
            [dict(source[0], charge=True)],
            [dict(source[0], element=7)],
            [dict(source[0], element="C", charge=0)],
        ):
            for format in ("cdxml", "cdx"):
                a, e = self.format_pair(actual, expected, format)
                with self.subTest(source=changed, format=format), self.assertRaises(ValueError):
                    compare(a, e, format, changed)
        actual, expected, source = self.zero_hydrogen_pair("C", 0)
        for format in ("cdxml", "cdx"):
            a, e = self.format_pair(actual, expected, format)
            with self.subTest(format=format), self.assertRaises(ValueError):
                compare(a, e, format, source)

    def test_zero_hydrogen_addition_preserves_all_other_fields_and_legacy_absence(self):
        actual, expected, source = self.zero_hydrogen_pair()
        for changed in (
            actual.replace('NumHydrogens="0"', 'NumHydrogens="1"'),
            actual.replace('Element="7"', 'Element="8"'),
            actual.replace('Charge="1"', 'Charge="2"'),
            actual.replace('p="1 2"', 'p="2 2"'),
            actual.replace('id="3"', 'id="4"'),
            actual.replace("<s>N</s>", "<s>NH</s>"),
            actual.replace("</fragment>", '<b id="4" B="3" E="3" Order="1"/></fragment>'),
            actual.replace('Element="7"', 'Element="7" color="3"'),
        ):
            for format in ("cdxml", "cdx"):
                a, e = self.format_pair(changed, expected, format)
                with self.subTest(actual=changed, format=format), self.assertRaises(ValueError):
                    compare(a, e, format, source)
        for value in ("00", "+0", "0.0", "", "-0"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                compare(
                    actual.replace('NumHydrogens="0"', f'NumHydrogens="{value}"'),
                    expected,
                    "cdxml",
                    source,
                )
        for value in ("0", "1"):
            legacy = expected.replace(' id="3"', f' id="3" NumHydrogens="{value}"')
            for format in ("cdxml", "cdx"):
                a, e = self.format_pair(actual, legacy, format)
                with self.subTest(legacy=value, format=format), self.assertRaises(ValueError):
                    compare(a, e, format, source)
        # CDX stores IDs as integers; this lexical mutation is distinguishable
        # only in CDXML, whose source-order IDs must use the writer's spelling.
        with self.assertRaises(ValueError):
            compare(
                actual.replace('id="3"', 'id="03"'),
                expected.replace('id="3"', 'id="03"'),
                "cdxml",
                source,
            )

    def test_zero_hydrogen_checks_every_atom_by_source_order_identity(self):
        expected = '<CDXML><page><fragment><n id="5" Element="1" Isotope="3"/><n id="4" Element="8"/><n id="3" Element="1" Isotope="2"/></fragment></page></CDXML>'
        actual = expected.replace('Element="1"', 'Element="1" NumHydrogens="0"')
        source = [
            dict(id=71, element="H", charge=0, no_implicit=True, explicit_h=0),
            dict(id=99, element="O", charge=0, no_implicit=False, explicit_h=0),
            dict(id=12, element="H", charge=0, no_implicit=True, explicit_h=0),
        ]
        self.compare_both_formats(actual, expected, source)
        # A neutral oxygen drawn between the two H nodes cannot borrow either
        # neighbor's independently captured no-implicit-H setting.
        for format in ("cdxml", "cdx"):
            a, e = self.format_pair(
                actual.replace('Element="8"', 'Element="8" NumHydrogens="0"'), expected, format
            )
            with self.subTest(format=format), self.assertRaises(ValueError):
                compare(a, e, format, source)
        # Duplicating an XML ID must not let a second atom borrow the first
        # source atom's qualifying semantics.
        for format in ("cdxml", "cdx"):
            duplicate_expected = expected.replace('id="4"', 'id="3"')
            duplicate_actual = actual.replace('id="4"', 'id="3"').replace(
                'Element="8"', 'Element="8" NumHydrogens="0"'
            )
            a, e = self.format_pair(duplicate_actual, duplicate_expected, format)
            with self.subTest(duplicate_id=True, format=format), self.assertRaises(ValueError):
                compare(a, e, format, source)

    def test_zero_hydrogen_diagnostics_are_uncapped_after_allowed_additions(self):
        count = 80
        nodes = "".join(f'<n id="{index + 3}" Element="8" p="0 0"/>' for index in range(count))
        expected = f"<CDXML><page><fragment>{nodes}</fragment></page></CDXML>"
        actual = expected.replace('Element="8"', 'Element="8" NumHydrogens="0" Charge="1"')
        source = [dict(element="O", charge=0, explicit_h=0, no_implicit=True) for _ in range(count)]
        for format in ("cdxml", "cdx"):
            a, e = self.format_pair(actual, expected, format)
            with self.subTest(format=format), self.assertRaises(ValueError) as result:
                compare(a, e, format, source)
            diagnostic = str(result.exception)
            self.assertIn(f"{count} mismatches (attribute={count})", diagnostic)
            self.assertEqual(count, diagnostic.count("@Charge:"))
            self.assertIn(f"/n[{count - 1}]@Charge", diagnostic)

    def dative_pair(self):
        expected = '<CDXML><page id="1"><fragment id="2"><n id="3" Element="7" p="0 0"/><n id="4" Element="29" Charge="2" p="28 0"/><b id="5" B="3" E="4" Order="1" Display="Dash" color="3" Z="-2" Display2="Solid" DoublePosition="Left"/></fragment></page></CDXML>'
        actual = expected.replace('Order="1"', 'Order="dative"')
        source = dict(
            atoms=[dict(id=9007199254740993), dict(id=18446744073709551615)],
            bonds=[dict(a=9007199254740993, b=18446744073709551615, order=5, display="dashed")],
        )
        return actual, expected, source

    def assert_dative_pair(self, actual, expected, source, accepted):
        for format in ("cdxml", "cdx"):
            a, e = self.format_pair(actual, expected, format)
            with self.subTest(format=format):
                if accepted:
                    compare(a, e, format, source_document=source)
                else:
                    with self.assertRaises(ValueError):
                        compare(a, e, format, source_document=source)

    def test_dative_export_accepts_only_independent_dashed_order_five_in_both_formats(self):
        actual, expected, source = self.dative_pair()
        self.assert_dative_pair(actual, expected, source, True)
        # Node chemistry is not used to discover the dative source bond.
        self.assert_dative_pair(
            actual.replace('Element="7"', 'Element="8"'),
            expected.replace('Element="7"', 'Element="8"'),
            source,
            True,
        )
        for field, values in (
            ("order", (None, 0, 1, 2, 3, 4, 6, 7, "5", 5.0, True)),
            ("display", (None, "plain", "bold", "dotted", "wedge", "Dash")),
        ):
            for value in values:
                changed = deepcopy(source)
                changed["bonds"][0][field] = value
                with self.subTest(field=field, value=value):
                    self.assert_dative_pair(actual, expected, changed, False)
            changed = deepcopy(source)
            del changed["bonds"][0][field]
            self.assert_dative_pair(actual, expected, changed, False)
        for changed in (None, {}, {"atoms": source["atoms"]}, dict(source, bonds=[])):
            self.assert_dative_pair(actual, expected, changed, False)

    def test_dative_export_requires_exact_legacy_and_actual_orders(self):
        actual, expected, source = self.dative_pair()
        for order in ("1", "2", "3", "4", "1.5", "hydrogen", "ionic"):
            with self.subTest(actual_order=order):
                self.assert_dative_pair(
                    actual.replace('Order="dative"', f'Order="{order}"'), expected, source, False
                )
        for order in ("2", "3", "4", "1.5", "dative", "hydrogen", "ionic"):
            with self.subTest(legacy_order=order):
                self.assert_dative_pair(
                    actual, expected.replace('Order="1"', f'Order="{order}"'), source, False
                )
        self.assert_dative_pair(actual.replace(' Order="dative"', ""), expected, source, False)
        self.assert_dative_pair(actual, expected.replace(' Order="1"', ""), source, False)
        # CDX enums have canonical spellings; lexical variants remain CDXML guards.
        for old, new in (("1", "01"), ("1", "+1"), ("dative", "Dative")):
            with self.subTest(old=old, new=new), self.assertRaises(ValueError):
                compare(
                    actual.replace(f'Order="{old}"', f'Order="{new}"'),
                    expected.replace(f'Order="{old}"', f'Order="{new}"'),
                    "cdxml",
                    source_document=source,
                )

    def test_dative_export_preserves_endpoints_display_and_every_other_attribute(self):
        actual, expected, source = self.dative_pair()
        for old, new in (
            ('B="3"', 'B="4"'),
            ('E="4"', 'E="3"'),
            ('id="5"', 'id="6"'),
            ('Display="Dash"', 'Display="Solid"'),
            ('Display2="Solid"', 'Display2="Dash"'),
            ('DoublePosition="Left"', 'DoublePosition="Right"'),
            ('color="3"', 'color="4"'),
            ('Z="-2"', 'Z="-1"'),
            ('Element="7"', 'Element="8"'),
            ('p="28 0"', 'p="29 0"'),
            ('id="3"', 'id="9"'),
            ('color="3"', 'color="3" IgnoreWarnings="yes"'),
            (' Display="Dash"', ""),
        ):
            with self.subTest(old=old, new=new):
                self.assert_dative_pair(actual.replace(old, new), expected, source, False)
        # Identical, forged endpoints on both sides still lack source association.
        for old, new in (('B="3"', 'B="4"'), ('id="5"', 'id="6"')):
            self.assert_dative_pair(
                actual.replace(old, new), expected.replace(old, new), source, False
            )
        # Other bond attributes must be lexically exact despite numeric equality.
        with self.assertRaises(ValueError):
            compare(
                actual.replace('Z="-2"', 'Z="-02"'),
                expected,
                "cdxml",
                source_document=source,
            )
        # A CDX object header always contains an ID; the legacy encoder supplies
        # a missing XML ID. Check missing lexical identity directly in CDXML.
        with self.assertRaises(ValueError):
            compare(
                actual.replace(' id="5"', ""),
                expected,
                "cdxml",
                source_document=source,
            )
        # Mutate the actual binary header, bypassing the old encoder's repair
        # of absent/zero XML IDs, to prove zero CDX identity also fails.
        binary, legacy = self.format_pair(actual, expected, "cdx")
        tampered = base64.b64decode(binary).replace(
            struct.pack("<HI", 0x8005, 5), struct.pack("<HI", 0x8005, 0)
        )
        self.assertNotEqual(tampered, base64.b64decode(binary))
        with self.assertRaises(ValueError):
            compare(
                base64.b64encode(tampered).decode(),
                legacy,
                "cdx",
                source_document=source,
            )

    def test_dative_export_rejects_missing_and_duplicate_source_identities(self):
        actual, expected, source = self.dative_pair()
        controls = [dict(source, atoms=[]), dict(source, atoms=None)]
        for value in (None, 0, True, "9007199254740993", 2**64, source["atoms"][1]["id"]):
            changed = deepcopy(source)
            changed["atoms"][0]["id"] = value
            controls.append(changed)
        changed = deepcopy(source)
        del changed["atoms"][0]["id"]
        controls.append(changed)
        for field in ("a", "b"):
            for value in (None, True, "9007199254740993", 17, source["bonds"][0]["a"]):
                changed = deepcopy(source)
                changed["bonds"][0][field] = value
                if changed != source:
                    controls.append(changed)
            changed = deepcopy(source)
            del changed["bonds"][0][field]
            controls.append(changed)
        for reversed in (False, True):
            changed = deepcopy(source)
            duplicate = dict(changed["bonds"][0], order=1)
            if reversed:
                duplicate["a"], duplicate["b"] = duplicate["b"], duplicate["a"]
            changed["bonds"].append(duplicate)
            controls.append(changed)
        for changed in controls:
            with self.subTest(source=changed):
                self.assert_dative_pair(actual, expected, changed, False)
        # Ambiguous output identities cannot receive the source bond's allowance.
        for duplicate in (
            '<b id="5" B="3" E="4" Order="1" Display="Dash"/>',
            '<n id="3" Element="7"/>',
        ):
            self.assert_dative_pair(
                actual.replace("</fragment>", duplicate + "</fragment>"),
                expected.replace("</fragment>", duplicate + "</fragment>"),
                source,
                False,
            )

    def test_dative_export_checks_source_bond_index_and_direction_independently(self):
        # Permute output order to ensure the source index IDs establish identity.
        expected = '<CDXML><page><fragment><n id="5" Element="29"/><n id="3" Element="7"/><n id="4" Element="6"/><b id="7" B="3" E="5" Order="1" Display="Dash"/><b id="6" B="3" E="4" Order="1" Display="Solid"/></fragment></page></CDXML>'
        actual = expected.replace('E="5" Order="1"', 'E="5" Order="dative"')
        source = dict(
            atoms=[dict(id=81), dict(id=19), dict(id=4)],
            bonds=[
                dict(a=81, b=19, order=1, display="plain"),
                dict(a=81, b=4, order=5, display="dashed"),
            ],
        )
        self.assert_dative_pair(actual, expected, source, True)
        changed = deepcopy(source)
        changed["bonds"].reverse()
        self.assert_dative_pair(actual, expected, changed, False)
        changed = deepcopy(source)
        changed["bonds"][1]["a"], changed["bonds"][1]["b"] = 4, 81
        self.assert_dative_pair(actual, expected, changed, False)

    def test_only_captured_arrow_tag_and_fill_codes_are_adjusted(self):
        text = '<CDXML><page id="1"><arrow id="2" Head3D="30 0 0" Tail3D="0 0 0" FillType="None"/><graphic id="3" GraphicType="Rectangle" FillType="Solid" BoundingBox="1 2 3 4"/></page></CDXML>'
        # Fixed 51fa0991 writer output for this XML: arrow 0x8027 and
        # FillType None=0 / Solid=1. Do not regenerate it with today's codec.
        legacy = bytes.fromhex(
            "566a4344303130300403020100000000000000000000008000000000018001000000"
            "27800200000007020c0000001e00000000000000000008020c000000000000000000"
            "00000000370a020000000000078003000000000a02000300370a0200010004021000"
            "000002000000010000000400000003000000000000000000"
        )
        # Independently patch the historical binary writer's exact codes;
        # neither the current Rust encoder nor the corrected decoder writes it.
        actual = legacy.replace(struct.pack("<HI", 0x8027, 2), struct.pack("<HI", 0x8021, 2))
        actual = actual.replace(struct.pack("<HHh", 0xA37, 2, 1), struct.pack("<HHh", 0xA37, 2, 2))
        actual = actual.replace(struct.pack("<HHh", 0xA37, 2, 0), struct.pack("<HHh", 0xA37, 2, 1))

        def encoded(data):
            return base64.b64encode(data).decode()

        # compare() deliberately reads its expected bytes with the historical
        # mapping, while current_cdx() temporarily installs the corrected one.
        # Scope the old decoder table to this legacy test and restore it after.
        historical_fill = ("FillType", "INT16", {"None": 0, "Solid": 1, "Shaded": 2})
        with patch.dict(PROPERTIES, {0xA37: historical_fill}):
            compare(encoded(actual), encoded(legacy), "cdx")
            for changed in (
                actual.replace(struct.pack("<HHh", 0xA37, 2, 2), struct.pack("<HHh", 0xA37, 2, 4)),
                actual.replace(struct.pack("<HI", 0x8021, 2), struct.pack("<HI", 0x8021, 4)),
                to_cdx(text.replace('Head3D="30 0 0"', 'Head3D="31 0 0"')),
                to_cdx(text.replace('FillType="None"', 'FillType="Solid"')),
            ):
                with self.subTest(binary=changed), self.assertRaises(ValueError):
                    compare(encoded(changed), encoded(legacy), "cdx")


if __name__ == "__main__":
    unittest.main()
