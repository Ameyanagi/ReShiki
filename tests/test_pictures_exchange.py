"""Editable embedded pictures alongside chemistry, including independent files."""

import base64
import io
import struct
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from PIL import Image

from engine.cdx_exchange import from_cdx, property_bytes, to_cdx
from engine.pictures_exchange import decode, read_picture
from engine.worker import handle

ROOT = Path(__file__).resolve().parents[1]


def raster(fmt="PNG", size=(12, 8)):
    pixels = Image.new("RGBA", size)
    for y in range(size[1]):
        for x in range(size[0]):
            pixels.putpixel(
                (x, y),
                [(255, 0, 0, 255), (0, 255, 0, 255), (0, 0, 255, 255), (0, 0, 0, 0)][
                    int(x >= size[0] / 2) + 2 * int(y >= size[1] / 2)
                ],
            )
    out = io.BytesIO()
    (pixels.convert("RGB") if fmt == "JPEG" else pixels).save(out, format=fmt)
    return out.getvalue()


def imported(xml):
    return handle(dict(protocol=1, operation="import", format="cdxml", text=xml))["document"]


def wrap(data, fmt="PNG", **attrs):
    root = ET.Element("CDXML", BondLength="14.4")
    page = ET.SubElement(root, "page", id="1")
    ET.SubElement(
        page, "embeddedobject", id="2", **{"BoundingBox": "10 20 82 56", fmt: data.hex(), **attrs}
    )
    return ET.tostring(root, encoding="unicode")


class PictureExchangeTests(unittest.TestCase):
    def test_raster_representations_and_native_fixed_point_rotation(self):
        for fmt in ["PNG", "TIFF", "JPEG", "GIF", "BMP"]:
            with self.subTest(fmt=fmt):
                xml = wrap(raster(fmt), fmt, RotationAngle=str(90 * 65536))
                for value in [xml, from_cdx(to_cdx(xml))]:
                    g = imported(value)["graphics"][0]
                    pixels = decode(base64.b64decode(g["picture"]))
                    self.assertEqual(pixels.size, (12, 8))
                    self.assertGreater(pixels.getpixel((0, 0))[0], 240)
                    self.assertAlmostEqual(g["axis_x"]["x"], 0)
                    self.assertAlmostEqual(g["axis_x"]["y"], 210, places=3)
                    self.assertAlmostEqual(g["axis_y"]["x"], -105, places=3)
                    self.assertAlmostEqual(g["axis_y"]["y"], 0)
                    self.assertAlmostEqual(g["origin"]["x"], 46 * 42 / 14.4 + 52.5, places=3)
                    self.assertAlmostEqual(g["origin"]["y"], 38 * 42 / 14.4 - 105, places=3)
                    if fmt in ("PNG", "TIFF"):
                        self.assertEqual(pixels.getpixel((11, 7))[3], 0)

    def test_raw_native_property_and_extended_blob_lengths(self):
        # Independent byte construction: the codec must not multiply the angle
        # by 65536 a second time, nor truncate an extended property length.
        blob = raster("TIFF", (160, 160))
        self.assertGreater(len(blob), 65535)
        data = (
            b"VjCD0100\x04\x03\x02\x01"
            + bytes(10)
            + struct.pack("<HIHIHI", 0x8000, 0, 0x8001, 1, 0x8009, 2)
            + property_bytes(
                0x0204, struct.pack("<iiii", 20 * 65536, 10 * 65536, 56 * 65536, 82 * 65536)
            )
            + property_bytes(0x0205, struct.pack("<i", 30 * 65536))
            + property_bytes(0x0A6F, blob)
            + b"\0\0" * 4
        )
        root = ET.fromstring(from_cdx(data))
        pic = root.find(".//embeddedobject")
        self.assertEqual(pic.get("RotationAngle"), str(30 * 65536))
        self.assertEqual(bytes.fromhex(pic.get("TIFF")), blob)
        self.assertIn(
            property_bytes(0x0205, struct.pack("<i", 30 * 65536)),
            to_cdx(ET.tostring(root, encoding="unicode")),
        )
        self.assertEqual(len(imported(from_cdx(data))["graphics"]), 1)

    def test_mixed_native_saved_picture_keeps_group_and_chemistry(self):
        data = (ROOT / "tests/fixtures/picture-group-native.cdx").read_bytes()
        result = handle(
            dict(protocol=1, operation="import", format="cdx", text=base64.b64encode(data).decode())
        )
        self.assertEqual(result["analysis"]["smiles"], "CCO")
        doc = result["document"]
        self.assertEqual(
            (len(doc["atoms"]), len(doc["bonds"]), len(doc["graphics"]), len(doc["groups"])),
            (3, 2, 1, 1),
        )
        self.assertEqual(
            set(doc["groups"][0]["members"]),
            {a["id"] for a in doc["atoms"]} | {doc["graphics"][0]["id"]},
        )
        g = doc["graphics"][0]
        self.assertEqual(g["kind"], "picture")
        self.assertEqual(decode(base64.b64decode(g["picture"])).size, (600, 360))
        self.assertAlmostEqual(g["axis_x"]["x"], 0, places=3)
        self.assertGreater(g["axis_x"]["y"], 0)

    def test_malformed_pixels_and_invalid_geometry_are_atomic_errors(self):
        for attrs in [
            {"PNG": "not-hex"},
            {"PNG": b"not an image".hex()},
            {"BoundingBox": "1 2 1 9"},
            {"BoundingBox": "1 nan 3 4"},
            {"RotationAngle": "nan"},
            {"alpha": "2"},
            {"TIFF": raster().hex(), "PNG": ""},
        ]:
            with self.subTest(attrs=attrs), self.assertRaises(ValueError):
                imported(wrap(raster(), **attrs))
        with self.assertRaisesRegex(ValueError, "vector/OLE-only"):
            imported(wrap(b"opaque", "PDF"))
        out = io.BytesIO()
        Image.new("RGB", (8193, 1)).save(out, format="PNG")
        with self.assertRaisesRegex(ValueError, "8192"):
            imported(wrap(out.getvalue()))
        # Worker remains usable after rejected data.
        self.assertEqual(len(imported(wrap(raster()))["graphics"]), 1)

    def test_picture_opacity_and_unsupported_duplicate_representations(self):
        g = imported(wrap(raster(), alpha=".5", PDF="00"))["graphics"][0]
        pixels = decode(base64.b64decode(g["picture"]))
        self.assertEqual(pixels.getpixel((0, 0))[3], 128)
        self.assertEqual(pixels.getpixel((11, 7))[3], 0)

    def test_document_picture_budget_stops_before_decoding_excess_pixels(self):
        el = ET.fromstring(wrap(raster())).find(".//embeddedobject")
        budget = dict(bytes=0, pixels=64_000_000 - 96)
        read_picture(el, 1, budget)
        with self.assertRaisesRegex(ValueError, "64 million"):
            read_picture(el, 1, budget)
        with self.assertRaisesRegex(ValueError, "64 MB"):
            read_picture(el, 1, dict(bytes=64 * 1024 * 1024, pixels=0))

    def test_export_margin_accounts_for_rotated_picture_and_relative_position(self):
        doc = handle(dict(protocol=1, operation="import", format="smiles", text="CCO"))["document"]
        g = imported(wrap(raster(), RotationAngle=str(37 * 65536)))["graphics"][0]
        g.update(id=100, origin=dict(x=-500, y=-600))
        doc["graphics"] = [g]
        doc["groups"] = [
            dict(id=101, members=[a["id"] for a in doc["atoms"]] + [100], integral=True)
        ]
        for fmt in ["cdxml", "cdx"]:
            output = handle(dict(protocol=1, operation="export", format=fmt, document=doc))[
                "output"
            ]
            response = handle(dict(protocol=1, operation="import", format=fmt, text=output))
            back = response["document"]
            restored = back["graphics"][0]
            self.assertEqual(response["analysis"]["smiles"], "CCO")
            self.assertTrue(back["groups"][0]["integral"])
            for axis in ["x", "y"]:
                self.assertAlmostEqual(
                    g["origin"][axis] - doc["atoms"][0]["position"][axis],
                    restored["origin"][axis] - back["atoms"][0]["position"][axis],
                    places=3,
                )
                corners = [
                    restored["origin"][axis]
                    + u * restored["axis_x"][axis]
                    + v * restored["axis_y"][axis]
                    for u, v in [(0, 0), (1, 0), (0, 1), (1, 1)]
                ]
                self.assertAlmostEqual(min(corners) * 14.4 / 42, 30, places=3)
            self.assertEqual(
                decode(base64.b64decode(g["picture"])).tobytes(),
                decode(base64.b64decode(restored["picture"])).tobytes(),
            )


if __name__ == "__main__":
    unittest.main()
