"""Binary interchange tests, including an independently produced clipboard fixture."""

import base64
import struct
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from engine.cdx_exchange import PROPERTIES, from_cdx, numeric, property_bytes, to_cdx
from engine.worker import handle

ROOT = Path(__file__).resolve().parents[1]


class BinaryDrawingTests(unittest.TestCase):
    def test_unknown_atom_stereo_matches_native_cdx_without_inventing_assignment(self):
        data = (ROOT / "tests/fixtures/chemdraw-unknown-atom-stereo/native.cdx").read_bytes()
        decoded = from_cdx(data)
        atom = ET.fromstring(decoded).find(".//n[@id='4']")
        self.assertEqual(atom.get("AS"), "?")
        restored = ET.fromstring(from_cdx(to_cdx(decoded)))
        self.assertEqual(restored.find(".//n[@id='4']").get("AS"), "?")
        name, kind, enum = PROPERTIES[0x437]
        for value, number in [
            ("U", 0),
            ("N", 1),
            ("R", 2),
            ("S", 3),
            ("r", 4),
            ("s", 5),
            ("u", 6),
            ("?", 12),
        ]:
            with self.subTest(value=value):
                payload = bytes([number])
                self.assertEqual(numeric(name, kind, enum, value), payload)
                self.assertEqual(numeric(name, kind, enum, payload, False), value)
        with self.assertRaisesRegex(ValueError, "Unsupported binary enumeration: AS"):
            numeric(name, kind, enum, b"\x09", False)
        with self.assertRaisesRegex(ValueError, "Unsupported drawing value: AS"):
            numeric(name, kind, enum, "unknown")

    def test_dipole_flag_is_implied_and_old_byte_flags_still_read(self):
        plain = to_cdx("<CDXML><page id='1'><arrow id='2'/></page></CDXML>")
        self.assertEqual(len(plain), 48)
        expected = plain[:40] + property_bytes(0xA3A, b"") + plain[40:]
        self.assertEqual(
            to_cdx("<CDXML><page id='1'><arrow id='2' Dipole='yes'/></page></CDXML>"),
            expected,
        )
        self.assertEqual(
            to_cdx("<CDXML><page id='1'><arrow id='2' Dipole='no'/></page></CDXML>"),
            plain,
        )
        for payload, flag in [(b"", "yes"), (b"\1", "yes"), (b"\0", "no")]:
            with self.subTest(payload=payload):
                old = plain[:40] + property_bytes(0xA3A, payload) + plain[40:]
                decoded = from_cdx(old)
                self.assertEqual(ET.fromstring(decoded).find(".//arrow").get("Dipole"), flag)
                self.assertEqual(to_cdx(decoded), expected if flag == "yes" else plain)

    def test_native_clipboard_fixture_has_expected_structure(self):
        data = (ROOT / "tests/fixtures/native-ethyl-clipboard.cdx").read_bytes()
        result = handle(
            dict(protocol=1, operation="import", format="cdx", text=base64.b64encode(data).decode())
        )
        self.assertEqual(result["analysis"]["smiles"], "CCOC(=O)c1ccccc1")
        self.assertEqual(len(result["document"]["atoms"]), 11)
        self.assertEqual(len(result["document"]["bonds"]), 11)

    def test_stereo_charge_isotope_and_bond_colors_survive_binary_exchange(self):
        for smiles in ["N[C@@H](C)C(=O)O", "[13CH3][NH3+]", "F/C=C/F", "[CH3]"]:
            with self.subTest(smiles=smiles):
                original = handle(
                    dict(protocol=1, operation="import", format="smiles", text=smiles)
                )
                for bond in original["document"]["bonds"]:
                    bond["color"] = [180, 50, 55]
                encoded = handle(
                    dict(
                        protocol=1, operation="export", format="cdx", document=original["document"]
                    )
                )["output"]
                result = handle(dict(protocol=1, operation="import", format="cdx", text=encoded))
                self.assertEqual(original["analysis"]["smiles"], result["analysis"]["smiles"])
                self.assertTrue(
                    all(b["color"] == [180, 50, 55] for b in result["document"]["bonds"])
                )

    def test_multibyte_style_boundaries_and_extended_text_lengths(self):
        root = ET.fromstring(
            '<CDXML><fonttable><font id="3" name="Arial"/></fonttable><page id="1"><t id="2"><s font="3" size="10" color="3">ΔG°\n</s><s font="3" face="1" size="12.5" color="4">日本語 → β</s></t></page></CDXML>'
        )
        long = ET.SubElement(root.find("page"), "t", id="4")
        ET.SubElement(long, "s", font="3").text = "x" * 65530
        encoded = to_cdx(ET.tostring(root, encoding="unicode"))
        restored = ET.fromstring(from_cdx(encoded))
        spans = restored.find(".//t").findall("s")
        self.assertEqual([s.text for s in spans], ["ΔG°\n", "日本語 → β"])
        self.assertEqual(spans[1].get("face"), "1")
        self.assertEqual(float(spans[1].get("size")), 12.5)
        self.assertEqual(restored.findall(".//t")[1].find("s").text, "x" * 65530)

    def test_corrupt_and_truncated_data_never_yields_a_partial_drawing(self):
        data = to_cdx(
            '<CDXML><page id="1"><fragment id="2"><n id="3" p="0 0" Element="8"/></fragment></page></CDXML>'
        )
        for end in range(len(data) - 2):
            with self.subTest(end=end), self.assertRaises(ValueError):
                from_cdx(data[:end])
        with self.assertRaisesRegex(ValueError, "header"):
            from_cdx(b"wrong" + data)
        with self.assertRaisesRegex(ValueError, "trailing"):
            from_cdx(data + b"junk")
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            from_cdx(to_cdx('<CDXML><page id="1"><n id="1" p="0 0"/></page></CDXML>'))
        header = data[:22]
        unsupported = (
            header
            + struct.pack("<HI", 0x8000, 0)
            + struct.pack("<HI", 0x8004, 1)
            + property_bytes(0x7777, b"")
            + b"\0\0" * 3
        )
        with self.assertRaisesRegex(ValueError, "Unsupported binary drawing property"):
            from_cdx(unsupported)

    def test_worker_rejects_invalid_base64_and_recovers_for_next_request(self):
        with self.assertRaises(ValueError):
            handle(dict(protocol=1, operation="import", format="cdx", text="!!!"))
        self.assertEqual(
            handle(dict(protocol=1, operation="import", format="smiles", text="CCO"))["analysis"][
                "formula"
            ],
            "C2H6O",
        )

    def test_query_predicates_are_not_silently_reduced_to_ordinary_atoms(self):
        for name, value in [
            ("RingBondCount", "NoRingBonds"),
            ("SubstituentsExactly", "0"),
            ("RxnStereo", "Inversion"),
        ]:
            xml = f'<CDXML><page id="1"><fragment id="2"><n id="3" p="0 0" Element="6" {name}="{value}"/></fragment></page></CDXML>'
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "predicate"):
                handle(
                    dict(
                        protocol=1,
                        operation="import",
                        format="cdx",
                        text=base64.b64encode(to_cdx(xml)).decode(),
                    )
                )


if __name__ == "__main__":
    unittest.main()
