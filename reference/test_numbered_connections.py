"""Native numbered-attachment codec regression, using original controlled data."""

import struct
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from engine.cdx_exchange import from_cdx, property_bytes, to_cdx

ROOT = Path(__file__).resolve().parents[1]


class NumberedConnectionTests(unittest.TestCase):
    def test_native_numbers_and_explicit_id_pairing_round_trip(self):
        for name, numbers in [("single", ["1"]), ("two", ["2", "1"]), ("sparse", ["3", "1"])]:
            with self.subTest(name=name):
                source = (
                    ROOT / "tests/fixtures/numbered-attachments" / (name + ".cdx")
                ).read_bytes()
                decoded = from_cdx(source)
                restored = ET.fromstring(from_cdx(to_cdx(decoded)))
                self.assertEqual(
                    [
                        node.get("ExternalConnectionNum")
                        for node in restored.iter()
                        if node.get("ExternalConnectionNum") is not None
                    ],
                    numbers,
                )
                if len(numbers) == 2:
                    self.assertEqual(
                        restored.find(".//n[@NodeType='Fragment']").get("BondOrdering"), "32 31"
                    )
                    self.assertEqual(
                        restored.find(".//fragment[@ConnectionOrder]").get("ConnectionOrder"),
                        "14 15",
                    )

    def test_positive_signed_byte_boundaries_round_trip(self):
        for number in (1, 12, 127):
            with self.subTest(number=number):
                source = (
                    '<CDXML><n NodeType="ExternalConnectionPoint" '
                    f'ExternalConnectionNum="{number}"/></CDXML>'
                )
                restored = ET.fromstring(from_cdx(to_cdx(source)))
                node = restored.find("n")
                self.assertEqual(node.get("NodeType"), "ExternalConnectionPoint")
                self.assertEqual(node.get("ExternalConnectionNum"), str(number))

    def test_malformed_signed_byte_and_wrong_node_are_rejected(self):
        header = to_cdx("<CDXML/>")[:22]

        def frame(number, node_type=12, repeat=False, node_code=0x8004, repeat_type=False):
            kind = (
                property_bytes(0x400, struct.pack("<H", node_type))
                if node_type is not None
                else b""
            )
            return (
                header
                + struct.pack("<HIHI", 0x8000, 0, node_code, 1)
                + property_bytes(0x44B, number)
                + (property_bytes(0x44B, number) if repeat else b"")
                + kind
                + (kind if repeat_type else b"")
                + b"\0\0" * 3
            )

        for number in (1, 12, 127):
            with self.subTest(binary_number=number):
                self.assertIn(
                    f'ExternalConnectionNum="{number}"',
                    from_cdx(frame(struct.pack("b", number))),
                )
        for number in (b"", b"\1\0"):
            with (
                self.subTest(number=number),
                self.assertRaisesRegex(
                    ValueError, "^Invalid binary number: ExternalConnectionNum$"
                ),
            ):
                from_cdx(frame(number))
        for number in (b"\0", b"\xff", b"\x80"):
            with (
                self.subTest(number=number),
                self.assertRaisesRegex(
                    ValueError, "^ExternalConnectionNum must be a positive signed byte$"
                ),
            ):
                from_cdx(frame(number))
        for node_code, kind in ((0x8004, 1), (0x8001, 12), (0x8005, 12)):
            with (
                self.subTest(node_code=node_code, kind=kind),
                self.assertRaisesRegex(
                    ValueError, "^ExternalConnectionNum requires an external connection point$"
                ),
            ):
                from_cdx(frame(b"\1", kind, node_code=node_code))
        with self.assertRaisesRegex(ValueError, "unique"):
            from_cdx(frame(b"\1", repeat=True))
        with self.assertRaisesRegex(ValueError, "unique"):
            from_cdx(frame(b"\1", repeat_type=True))
        with self.assertRaisesRegex(ValueError, "unique"):
            from_cdx(frame(b"\1", node_type=None))
        for tag, kind in (
            ("n", None),
            ("n", "Element"),
            ("n", "Fragment"),
            ("page", "ExternalConnectionPoint"),
            ("b", "ExternalConnectionPoint"),
        ):
            with (
                self.subTest(tag=tag, kind=kind),
                self.assertRaisesRegex(
                    ValueError, "^ExternalConnectionNum requires an external connection point$"
                ),
            ):
                owner = f' NodeType="{kind}"' if kind is not None else ""
                to_cdx(f'<CDXML><{tag}{owner} ExternalConnectionNum="1"/></CDXML>')
        for number in ("0", "-1", "-128", "128", "255", "1.5", "", "NaN", "inf"):
            with (
                self.subTest(number=number),
                self.assertRaisesRegex(
                    ValueError, "^ExternalConnectionNum must be a positive signed byte$"
                ),
            ):
                to_cdx(
                    '<CDXML><n NodeType="ExternalConnectionPoint" '
                    f'ExternalConnectionNum="{number}"/></CDXML>'
                )


if __name__ == "__main__":
    unittest.main()
