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

    def test_malformed_signed_byte_and_wrong_node_are_rejected(self):
        header = to_cdx("<CDXML/>")[:22]

        def frame(number, node_type=12, repeat=False):
            return (
                header
                + struct.pack("<HIHI", 0x8000, 0, 0x8004, 1)
                + property_bytes(0x44B, number)
                + (property_bytes(0x44B, number) if repeat else b"")
                + property_bytes(0x400, struct.pack("<H", node_type))
                + b"\0\0" * 3
            )

        self.assertIn('ExternalConnectionNum="1"', from_cdx(frame(b"\1")))
        for number, kind in [
            (b"", 12),
            (b"\1\0", 12),
            (b"\0", 12),
            (b"\xff", 12),
            (b"\x80", 12),
            (b"\1", 1),
        ]:
            with self.subTest(number=number, kind=kind), self.assertRaises(ValueError):
                from_cdx(frame(number, kind))
        with self.assertRaisesRegex(ValueError, "unique"):
            from_cdx(frame(b"\1", repeat=True))
        for kind, number in [
            ("Element", "1"),
            ("ExternalConnectionPoint", "0"),
            ("ExternalConnectionPoint", "128"),
            ("ExternalConnectionPoint", "1.5"),
        ]:
            with self.subTest(kind=kind, number=number), self.assertRaises(ValueError):
                to_cdx(f'<CDXML><n NodeType="{kind}" ExternalConnectionNum="{number}"/></CDXML>')


if __name__ == "__main__":
    unittest.main()
