"""The legacy comparison adjustment must not hide chemical or layout differences."""

import base64
import struct
import unittest
import xml.etree.ElementTree as ET

from reference_presentation import compare

from engine.cdx_exchange import to_cdx


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

    def test_only_captured_arrow_tag_and_fill_codes_are_adjusted(self):
        text = '<CDXML><page id="1"><arrow id="2" Head3D="30 0 0" Tail3D="0 0 0" FillType="None"/><graphic id="3" GraphicType="Rectangle" FillType="Solid" BoundingBox="1 2 3 4"/></page></CDXML>'
        legacy = to_cdx(text)
        # Independently patch the historical binary writer's exact codes;
        # neither the current Rust encoder nor the corrected decoder writes it.
        actual = legacy.replace(struct.pack("<HI", 0x8027, 2), struct.pack("<HI", 0x8021, 2))
        actual = actual.replace(struct.pack("<HHh", 0xA37, 2, 1), struct.pack("<HHh", 0xA37, 2, 2))
        actual = actual.replace(struct.pack("<HHh", 0xA37, 2, 0), struct.pack("<HHh", 0xA37, 2, 1))

        def encoded(data):
            return base64.b64encode(data).decode()

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
