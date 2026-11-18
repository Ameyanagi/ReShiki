"""Differential corpus for the Rust codec; Python stays an independent oracle.

Invoked by reference/cdx_codec.rs. This script never regenerates expected results
with Rust, so a migration cannot make both sides agree by accident. Arrow/fill
compatibility corrections are grounded by GUI-captured ChemDraw binary/XML
pairs in fixtures/chemdraw-captions and fixtures/chemdraw-arrows. UTF8Text
supplies an explicit charset for UTF-8 payloads. The worker's codec decodes
every point, rectangle and curve point as exact signed 16.16 values.
"""

from __future__ import annotations

import base64
import json
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import engine.cdx_exchange as legacy_codec
from engine.cdx_exchange import BY_NAME, INTS, OBJECTS, PROPERTIES, property_bytes
from engine.cdx_exchange import encode_text as legacy_encode_text
from engine.cdx_exchange import from_cdx as legacy_from_cdx
from engine.cdx_exchange import to_cdx as legacy_to_cdx

# Genuine ChemDraw 26 files in fixtures/chemdraw-arrows establish these codes.
# Keep the historical worker untouched. Its published-SDK arrow code is wrong;
# the decoder still accepts old ReShiki files, and the encoder's reverse mapping
# selects the canonical entry appended here. Scheme/step preserve references
# that ChemDraw automatically adds when saving drawings containing arrows.
OBJECTS.update({0x8021: "arrow", 0x800D: "scheme", 0x800E: "step"})
# The real files store FillType=None as 1. The published CDXConstants.h confirms
# Unspecified=0, None=1, Solid=2, Shaded=4; keep unsupported fills rejected.
fill_types = {"Unspecified": 0, "None": 1, "Solid": 2, "Shaded": 4}
PROPERTIES[0xA37] = ("FillType", "INT16", fill_types)
BY_NAME["FillType"] = (0xA37, "INT16", fill_types)

LINE_HEIGHTS = {"LineHeight", "CaptionLineHeight", "LabelLineHeight"}
COORDINATE_ATTRIBUTES = sorted(
    name
    for name, kind, _ in PROPERTIES.values()
    if kind in ("CDXPoint2D", "CDXPoint3D", "CDXRectangle", "CDXCurvePoints")
)
# An encoding-only alias uses the reference codec's existing explicit boolean
# encoder for false, while its ordinary implied-boolean behavior stays intact.
BY_NAME["_NonchemicalCaption"] = (0x0708, "CDXBoolean", {})


class Utf8TextPayload(bytes):
    """Distinguish styled text from metadata aliases sharing the Text tag."""


def text_adapter(element: ET.Element[str]) -> bytes:
    data = legacy_encode_text(element)
    runs = struct.unpack_from("<H", data)[0]
    text = data[2 + runs * 10 :]
    # UTF8Text is self-describing even without a font table. The legacy
    # encoder already writes UTF-8 bytes, but tags every text object as
    # charset-dependent Text. Only inspect its character payload: styled
    # run headers can contain high bytes for entirely ASCII text.
    return data if text.isascii() else Utf8TextPayload(data)


def text_property_adapter(code: int, data: bytes) -> bytes:
    if code == 0x700 and isinstance(data, Utf8TextPayload):
        code = 0x709
    return property_bytes(code, data)


# Keep these intentional interchange corrections in the independent adapter,
# outside the worker's codec in reference/engine/cdx_exchange.py.
setattr(legacy_codec, "encode_text", text_adapter)
setattr(legacy_codec, "property_bytes", text_property_adapter)


def to_cdx(text):
    root = ET.fromstring(text)
    for node in root.iter():
        if node.get("InterpretChemically") == "no":
            del node.attrib["InterpretChemically"]
            node.set("_NonchemicalCaption", "no")
        for name in LINE_HEIGHTS & node.attrib.keys():
            try:
                value = float(node.attrib[name])
            except ValueError:
                continue  # Preserve variable/automatic aliases.
            if value not in (0, 1):
                node.set(name, str(value * 20))
    return legacy_to_cdx(ET.tostring(root, encoding="unicode"))


def from_cdx(data):
    root = ET.fromstring(legacy_from_cdx(data))
    for node in root.iter():
        for name in LINE_HEIGHTS & node.attrib.keys():
            try:
                value = float(node.attrib[name])
            except ValueError:
                continue
            node.set(name, f"{value / 20:.8g}")
    return ET.tostring(root, encoding="unicode")


def coordinate_boundary_cases():
    header = b"VjCD0100\x04\x03\x02\x01" + bytes(10)
    cases = []
    for name, order in [
        ("p", (1, 0)),
        ("xyz", (0, 1, 2)),
        ("BoundingBox", (1, 0, 3, 2)),
        ("CurvePoints", (1, 0, 3, 2)),
    ]:
        for ticks in [
            (-(2**31), 2**31 - 1, -1, 1),
            (-1, 1, -(2**31), 2**31 - 1),
            (1431717, -1431717, 65535, -65535),
        ]:
            payload = struct.pack("<" + "i" * len(order), *(ticks[index] for index in order))
            if name == "CurvePoints":
                payload = struct.pack("<H", 2) + payload
            data = (
                header
                + struct.pack("<HIHI", 0x8000, 0, 0x8004, 1)
                + property_bytes(BY_NAME[name][0], payload)
                + bytes(6)
            )
            xml = from_cdx(data)
            node = ET.fromstring(xml).find("n")
            assert node is not None
            values = node.attrib[name].split()
            # Parsing the oracle XML must recover every original signed tick,
            # including +/- one tick and both signed 32-bit endpoints.
            actual_ticks = tuple(float(value) * 65536 for value in values)
            assert actual_ticks == ticks[: len(order)], (name, ticks, xml)
            cases.append((f"exact fixed-point {name} {ticks}", data))
    return cases


def check_adapters():
    cases = coordinate_boundary_cases()
    for text, tag in [("ASCII", 0x700), ("触媒 α", 0x709)]:
        # Font 300 and size 144 make the run header non-ASCII in both cases.
        xml = f'<CDXML><t id="1"><s font="300" size="144">{text}</s></t></CDXML>'
        data = to_cdx(xml)
        assert struct.unpack_from("<H", data, 34)[0] == tag
        span = ET.fromstring(from_cdx(data)).find("t/s")
        assert span is not None and span.text == text
    metadata = to_cdx('<CDXML><n id="1" temp_Text="café"/></CDXML>')
    assert struct.unpack_from("<H", metadata, 34)[0] == 0x700
    span = ET.fromstring(from_cdx(metadata)).find("n/s")
    assert span is not None and span.text == "café"
    return len(cases) + 3


def corpus():
    cases = []

    def binary(name, data):
        cases.append(
            dict(
                name=name,
                binary=base64.b64encode(data).decode(),
                decoded=from_cdx(data),
                coordinate_attributes=COORDINATE_ATTRIBUTES,
            )
        )

    def xml(name, text):
        data = to_cdx(text)
        binary(name, data)
        cases[-1]["xml"] = text

    for path in sorted((Path(__file__).resolve().parents[1] / "tests/fixtures").glob("*.cdx")):
        binary(path.name, path.read_bytes())
    for path in sorted((Path(__file__).resolve().parents[1] / "tests/fixtures").glob("*.cdxml")):
        # Native files can contain metadata rejected by both encoders; those
        # remain covered by the worker's CDXML import/export round-trip tests.
        try:
            xml(path.name, path.read_text())
        except (ValueError, UnicodeError):
            pass
    xml("empty", "<CDXML/>")
    for name, data in coordinate_boundary_cases():
        binary(name, data)
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
        if name == "ExternalConnectionNum":
            # Native numbered attachment points require this owner and a
            # positive signed byte. Wrong owners/ranges remain rejection
            # cases in test_numbered_connections.py, outside this corpus.
            values = ["1", "12", "127"]
        elif enum and kind in INTS:
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
            if name == "ExternalConnectionNum":
                node.set("NodeType", "ExternalConnectionPoint")
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
    if sys.argv[1:] == ["--check-adapters"]:
        print(f"CDX oracle adapter checks passed: {check_adapters()}")
    elif sys.argv[1:] == ["--roundtrip"]:
        binary = to_cdx(sys.stdin.read())
        print(json.dumps(dict(binary=base64.b64encode(binary).decode(), decoded=from_cdx(binary))))
    elif sys.argv[1:] == ["--decode"]:
        print(json.dumps(from_cdx(base64.b64decode(sys.stdin.read(), validate=True))))
    else:
        assert not sys.argv[1:]
        print(json.dumps(corpus(), ensure_ascii=False))
