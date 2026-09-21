"""Differential corpus for the Rust codec; Python stays an independent oracle.

Invoked by tests/cdx_codec.rs. This script never regenerates expected results
with Rust, so a migration cannot make both sides agree by accident.
"""

import base64
import json
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.cdx_exchange import INTS, PROPERTIES, from_cdx, property_bytes, to_cdx


def corpus():
    cases = []

    def binary(name, data):
        cases.append(
            dict(name=name, binary=base64.b64encode(data).decode(), decoded=from_cdx(data))
        )

    def xml(name, text):
        data = to_cdx(text)
        binary(name, data)
        cases[-1]["xml"] = text

    for path in sorted(Path(__file__).with_name("fixtures").glob("*.cdx")):
        binary(path.name, path.read_bytes())
    for path in sorted(Path(__file__).with_name("fixtures").glob("*.cdxml")):
        # Native files can contain metadata rejected by both encoders; those
        # remain covered by the worker's CDXML import/export round-trip tests.
        try:
            xml(path.name, path.read_text())
        except (ValueError, UnicodeError):
            pass
    xml("empty", "<CDXML/>")
    xml("implicit ids", "<CDXML><page><group><t><s>text</s></t></group></page></CDXML>")
    xml(
        "styled unicode",
        '<CDXML><fonttable><font id="3" name="ヒラギノ角ゴシック"/></fonttable>'
        '<page id="1"><t id="2"><s font="3">ΔG°\n</s>'
        '<s font="3" face="1" size="12.5" color="4">日本語 → β &amp; &lt;O&gt;</s>'
        '</t><t id="4"><s font="3">' + "x" * 65530 + "</s></t></page></CDXML>",
    )
    xml(
        "tables and references",
        '<CDXML><colortable><color r="0" g="0.5" b="1"/></colortable>'
        '<page id="1"><n id="2" p="-0.0000152587890625 32767.99998474121">'
        '<represent object="3" attribute="Element"/></n>'
        '<embeddedobject id="3" PNG="00 12 ab ff"/></page></CDXML>',
    )
    # Test every supported scalar and enumeration, including aliases and flags.
    for code, (name, kind, enum) in PROPERTIES.items():
        if name in ("fonttable", "colortable", "Text", "UTF8Text"):
            continue
        if enum and kind in INTS:
            values = list(enum)
        elif kind in INTS:
            values = ["0", "1", "12"]
            if kind.startswith("INT") or kind in ("CDXCoordinate", "FLOAT64"):
                values += ["-1"]
            if kind in ("CDXCoordinate", "FLOAT64") or name in ("BondSpacing", "ChainAngle"):
                values += ["0.125", "1.23456789", "0.0000001"]
        elif kind in ("CDXBoolean", "CDXBooleanImplied"):
            values = ["yes", "no"]
        elif kind in ("CDXPoint2D", "CDXPoint3D", "CDXRectangle"):
            count = {"CDXPoint2D": 2, "CDXPoint3D": 3, "CDXRectangle": 4}[kind]
            values = [" ".join(str(-i * 1.234567) for i in range(count))]
        elif kind == "CDXObjectIDArray":
            values = ["", "1 2 4294967295"]
        elif kind == "CDXCurvePoints":
            values = ["", "-10.25 0 2.5 100"]
        elif kind == "INT16ListWithCounts":
            values = ["", "0 1 65535"]
        elif kind == "CDXString":
            values = ["", "café & <O>"]
        else:
            continue
        for value in values:
            root = ET.Element("CDXML")
            node = ET.SubElement(ET.SubElement(root, "page", id="1"), "n", id="2")
            node.set(name, value)
            xml(f"property {code:x} {value}", ET.tostring(root, encoding="unicode"))
    # Legacy charsets, style boundaries, UTF-8 override, and both header sizes.
    header = b"VjCD0100\x04\x03\x02\x01" + bytes(10)
    for charset, encoding, text in [
        (65001, "utf-8", "日本語 → β"),
        (10000, "mac_roman", "café"),
        (1252, "cp1252", "€—"),
        (0, "latin-1", "ÿ"),
        (932, "cp932", "化学式"),
        (936, "gbk", "化学"),
        (949, "cp949", "화학"),
        (950, "big5", "化學"),
        (1251, "cp1251", "Химия"),
    ]:
        table = struct.pack("HHHHH", 0, 1, 3, charset, 5) + b"Arial"
        runs = struct.pack("HHHHHH", 1, 0, 3, 0, 200, 3) + text.encode(encoding)
        drawing = (
            struct.pack("<HI", 0x8000, 0)
            + property_bytes(0x100, table)
            + struct.pack("<HI", 0x8006, 1)
            + property_bytes(0x700, runs)
            + bytes(6)
        )
        binary(f"charset {charset}", header + drawing)
        binary(f"28 byte header {charset}", header + bytes(6) + drawing)

    def native_object(code, identifier, *props):
        return (
            struct.pack("<HI", code, identifier)
            + b"".join(property_bytes(tag, data) for tag, data in props)
            + bytes(2)
        )

    binary(
        "UTF8 property wins",
        header
        + native_object(0x8000, 0)[:-2]
        + native_object(
            0x8006,
            1,
            (0x700, b"invalid old text"),
            (0x709, struct.pack("<H", 0) + "日本語".encode()),
        )
        + bytes(4),
    )
    binary(
        "unstyled Latin-1 text",
        header
        + native_object(0x8000, 0)[:-2]
        + native_object(0x8006, 1, (0x700, bytes(2) + b"caf\xe9"))
        + bytes(4),
    )
    binary(
        "default styles",
        header
        + native_object(
            0x8000,
            0,
            (0x80A, struct.pack("<HHHH", 3, 1, 250, 4)),
            (0x80B, struct.pack("<HHHH", 3, 2, 200, 3)),
            (0xF, b""),
            (0x7777, b"bookkeeping"),
        )
        + bytes(2),
    )
    binary(
        "node bookkeeping",
        header
        + native_object(0x8000, 0)[:-2]
        + native_object(0x8004, 1, (0x448, b"unused"), (0x44D, b"unused"))
        + bytes(4),
    )
    for position in (0, 1, 2):
        binary(
            f"automatic double position {position}",
            header
            + native_object(0x8000, 0)[:-2]
            + native_object(0x8005, 1, (0x603, struct.pack("<H", position)))
            + bytes(4),
        )
    for code, (name, kind, enum) in PROPERTIES.items():
        if name in ("Order", "LineType", "RectangleType", "OvalType", "CurveType"):
            keys = [key for key, value in enum.items() if value]
            root = ET.Element("CDXML")
            node = ET.SubElement(root, "b", id="1")
            node.set(name, " ".join(keys[:2]))
            xml(f"combined flags {name}", ET.tostring(root, encoding="unicode"))
    # Every Python-defined single/double-byte character in the supported legacy
    # codepages, partitioned into bounded text objects. This catches differences
    # between Windows codepages and similarly named web encodings.
    for charset, encoding in [
        (10000, "mac_roman"),
        (1252, "cp1252"),
        (1251, "cp1251"),
        (932, "cp932"),
        (936, "gbk"),
        (949, "cp949"),
        (950, "big5"),
    ]:
        characters = []
        for code in range(65536 if charset in (932, 936, 949, 950) else 256):
            raw = bytes([code]) if code < 256 else code.to_bytes(2, "big")
            try:
                value = raw.decode(encoding)
            except UnicodeError:
                continue
            if len(value) == 1 and ord(value) >= 32 and ord(value) not in (0xFFFE, 0xFFFF):
                characters.append(raw)
        for start in range(0, len(characters), 8000):
            raw = b"".join(characters[start : start + 8000])
            table = struct.pack("<HHHHH", 0, 1, 3, charset, 5) + b"Arial"
            runs = struct.pack("<HHHHHH", 1, 0, 3, 0, 200, 3) + raw
            drawing = (
                struct.pack("<HI", 0x8000, 0)
                + property_bytes(0x100, table)
                + struct.pack("<HI", 0x8006, 1)
                + property_bytes(0x700, runs)
                + bytes(6)
            )
            binary(f"complete charset {charset} chunk {start}", header + drawing)
    return cases


if __name__ == "__main__":
    print(json.dumps(corpus(), ensure_ascii=False))
