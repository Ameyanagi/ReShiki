"""Bounded binary drawing interchange, using the published CDX tagged format.

The decoder produces CDXML for the existing checked drawing importer. No raster
fallback is used when editable data is present but unsupported.
"""

import math
import struct
import xml.etree.ElementTree as ET
from typing import TYPE_CHECKING

if TYPE_CHECKING or __package__:
    from .cdx_schema import PROPERTIES as PUBLISHED
else:
    from cdx_schema import PROPERTIES as PUBLISHED

LIMIT = 16 * 1024 * 1024
MAX_OBJECTS = 100_000
OBJECTS = {
    0x8000: "CDXML",
    0x8001: "page",
    0x8002: "group",
    0x8003: "fragment",
    0x8004: "n",
    0x8005: "b",
    0x8006: "t",
    0x8007: "graphic",
    0x8008: "curve",
    0x8009: "embeddedobject",
    0x8011: "objecttag",
    0x8027: "arrow",
}
PROPERTIES = dict(PUBLISHED)
# The overview predates these enumerations. Numeric values are cross-checked
# against the public SDK constants and native clipboard fixtures.
for code, enum in {
    0x602: {**PUBLISHED[0x601][2], "DottedHydrogen": 15},
    0xA2F: {"Solid": 1, "Hollow": 2, "Angle": 3},
    0xA35: {"None": 0, "Full": 2, "HalfLeft": 3, "HalfRight": 4},
    0xA36: {"None": 0, "Full": 2, "HalfLeft": 3, "HalfRight": 4},
    0xA37: {"None": 0, "Solid": 1, "Shaded": 2},
    0xA3B: {"None": 0, "Cross": 1, "Hash": 2},
}.items():
    name, kind, _ = PROPERTIES[code]
    PROPERTIES[code] = name, kind, enum
PROPERTIES[0x13] = ("SupersededBy", "CDXObjectID", {})
# The published overview duplicates 0xA38. Native closed-curve CDX uses
# 0xA39 with an empty payload; 0xA38 is the numeric CurveSpacing property.
PROPERTIES[0xA38] = ("CurveSpacing", "UINT16", {})
PROPERTIES[0xA39] = ("Closed", "CDXBooleanImplied", {})
BY_NAME = {v[0]: (k, v[1], v[2]) for k, v in PROPERTIES.items()}
INTS = {
    "INT8": "b",
    "UINT8": "B",
    "INT16": "h",
    "UINT16": "H",
    "INT32": "i",
    "UINT32": "I",
    "CDXObjectID": "I",
    "CDXCoordinate": "i",
    "FLOAT64": "d",
}
BITFIELDS = {"Order", "LineType", "RectangleType", "OvalType", "CurveType"}
# RotationAngle retains its 16.16 value in CDXML, unlike ChainAngle.
ANGLES = {"ChainAngle", "PositioningAngle"}
# Non-rendering bookkeeping on current native nodes. Other unknown properties
# on drawing objects are refused instead of silently discarding features.
NODE_BOOKKEEPING = {0x448, 0x44D}
ENCODINGS = {
    65001: "utf-8",
    10000: "mac_roman",
    1252: "cp1252",
    0: "latin-1",
    932: "cp932",
    936: "gbk",
    949: "cp949",
    950: "big5",
    1251: "cp1251",
}


class Reader:
    def __init__(self, data):
        self.data, self.position = data, 0

    def take(self, size):
        if size < 0 or size > len(self.data) - self.position:
            raise ValueError("Truncated binary drawing")
        start = self.position
        self.position += size
        return self.data[start : self.position]

    def number(self, kind):
        return struct.unpack("<" + kind, self.take(struct.calcsize("<" + kind)))[0]

    def done(self):
        if self.position != len(self.data):
            raise ValueError("Unexpected bytes in binary drawing property")


def packed(kind, *values):
    try:
        return struct.pack("<" + kind, *values)
    except (struct.error, OverflowError) as e:
        raise ValueError("Drawing value exceeds the binary format range") from e


def property_bytes(code, data):
    if len(data) > LIMIT:
        raise ValueError("Drawing property is too large")
    length = packed("H", len(data)) if len(data) < 65535 else packed("HI", 65535, len(data))
    return packed("H", code) + length + data


def coordinates(value, kind, encode=True):
    count = {"CDXPoint2D": 2, "CDXPoint3D": 3, "CDXRectangle": 4}[kind]
    if encode:
        values = [float(v) for v in value.split()]
        if len(values) != count or not all(math.isfinite(v) for v in values):
            raise ValueError("Invalid drawing coordinates")
        values = [round(v * 65536) for v in values]
    else:
        if len(value) != 4 * count:
            raise ValueError("Invalid binary coordinates")
        values = [v / 65536 for v in struct.unpack("<" + "i" * count, value)]
    if count == 2:
        values = [values[1], values[0]]
    if count == 4:
        values = [values[1], values[0], values[3], values[2]]
    return packed("i" * count, *values) if encode else " ".join(f"{v:.8g}" for v in values)


def numeric(name, kind, enum, value, encode=True):
    if name in ("LineHeight", "CaptionLineHeight", "LabelLineHeight"):
        enum = {"variable": 0, "auto": 1, "Variable": 0, "Auto": 1, "Automatic": 1}
    if encode:
        if value in enum:
            number = enum[value]
        elif name in BITFIELDS and enum:
            try:
                number = int(value)
            except ValueError:
                number = 0
                for part in value.split():
                    if part not in enum:
                        raise ValueError("Unsupported drawing flag: " + name)
                    number |= enum[part]
        elif enum and name not in ("LineHeight", "CaptionLineHeight", "LabelLineHeight"):
            raise ValueError("Unsupported drawing value: " + name + "=" + value)
        else:
            number = float(value)
        if kind == "CDXCoordinate" or name in ANGLES:
            number *= 65536
        if name == "BondSpacing":
            number *= 10
        if kind != "FLOAT64":
            number = round(number)
        if not math.isfinite(number):
            raise ValueError("Non-finite drawing value")
        # These flags use all 16 bits even though the original table calls them INT16.
        fmt = "H" if kind == "INT16" and name in BITFIELDS else INTS[kind]
        return packed(fmt, number)
    fmt = "H" if kind == "INT16" and name in BITFIELDS else INTS[kind]
    if len(value) != struct.calcsize(fmt):
        raise ValueError("Invalid binary number: " + name)
    number = struct.unpack("<" + fmt, value)[0]
    if not math.isfinite(number):
        raise ValueError("Non-finite binary number")
    if name == "DoublePosition" and number in (0, 1, 2):
        return "auto"
    if enum:
        reverse = {v: k for k, v in enum.items()}
        if number in reverse:
            value = reverse[number]
            return (
                value.lower()
                if name in ("LineHeight", "CaptionLineHeight", "LabelLineHeight")
                else value
            )
        if name in BITFIELDS:
            if name == "CurveType":
                return str(number)
            flags = [key for key, val in enum.items() if val and (number & val) == val]
            known = 0
            for key in flags:
                known |= enum[key]
            if known == number:
                return " ".join(flags)
        if name not in ("LineHeight", "CaptionLineHeight", "LabelLineHeight"):
            raise ValueError("Unsupported binary enumeration: " + name)
    if kind == "CDXCoordinate" or name in ANGLES:
        number /= 65536
    if name == "BondSpacing":
        number /= 10
    return f"{number:.8g}"


def encode_text(element):
    runs, parts, length = [], [], 0
    for span in element:
        if span.tag != "s":
            continue
        data = (span.text or "").replace("\n", "\r").encode("utf-8")
        runs.append(
            packed(
                "HHHHH",
                length,
                int(span.get("font", "3")),
                int(span.get("face", "0")),
                round(float(span.get("size", "10")) * 20),
                int(span.get("color", "3")),
            )
        )
        parts.append(data)
        length += len(data)
        if length > 65535:
            raise ValueError("A binary text object must be shorter than 65536 UTF-8 bytes")
    return packed("H", len(runs)) + b"".join(runs) + b"".join(parts)


def decode_text(data, element, fonts, utf8=False):
    reader = Reader(data)
    count = reader.number("H")
    runs = [tuple(reader.number("H") for _ in range(5)) for _ in range(count)]
    text = reader.take(len(data) - reader.position)
    if not runs:
        runs = [(0, None, 0, 0, 0)]
    offsets = [r[0] for r in runs] + [len(text)]
    if offsets[0] != 0 or any(a > b for a, b in zip(offsets, offsets[1:])):
        raise ValueError("Invalid binary text style ranges")
    for i, (start, font, face, size, color) in enumerate(runs):
        encoding = "utf-8" if utf8 else fonts.get(font, "latin-1")
        try:
            value = text[start : offsets[i + 1]].decode(encoding).replace("\r", "\n")
        except UnicodeError as e:
            raise ValueError("Invalid binary drawing text encoding") from e
        attrs = (
            {}
            if font is None
            else dict(font=str(font), face=str(face), size=str(size / 20), color=str(color))
        )
        ET.SubElement(element, "s", attrs).text = value


def to_cdx(xml):
    if len(xml.encode("utf-8")) > LIMIT:
        raise ValueError("Drawing exceeds the 16 MB structure limit")
    root = ET.fromstring(xml)
    if root.tag != "CDXML":
        raise ValueError("Expected a CDXML drawing")
    tags = {name: code for code, name in OBJECTS.items()}
    used = {int(el.get("id", "0")) for el in root.iter()}
    next_id = max(used, default=0) + 1
    count = 0

    def write(el, depth=0):
        nonlocal count, next_id
        count += 1
        if depth > 64 or count > MAX_OBJECTS:
            raise ValueError("Drawing object limit exceeded")
        if el.tag not in tags:
            raise ValueError("Unsupported binary drawing object: " + el.tag)
        identifier = int(el.get("id", "0"))
        if identifier == 0 and el is not root:
            identifier = next_id
            next_id += 1
        result = bytearray(packed("HI", tags[el.tag], identifier))
        for name, value in el.attrib.items():
            if name == "id":
                continue
            if name not in BY_NAME:
                raise ValueError("Unsupported binary drawing property: " + name)
            code, kind, enum = BY_NAME[name]
            if kind in INTS:
                data = numeric(name, kind, enum, value)
            elif kind == "CDXBooleanImplied" and value == "no":
                continue
            elif kind == "CDXBooleanImplied" and value == "yes":
                data = b""
            elif kind in ("CDXBoolean", "CDXBooleanImplied"):
                if value not in ("yes", "no"):
                    raise ValueError("Invalid drawing boolean")
                data = bytes([value == "yes"])
            elif kind in ("CDXPoint2D", "CDXPoint3D", "CDXRectangle"):
                data = coordinates(value, kind)
            elif kind == "CDXString":
                data = b"\0\0" + value.encode("latin-1")
            elif kind == "CDXObjectIDArray":
                data = b"".join(packed("I", int(v)) for v in value.split())
            elif kind == "Unformatted" and el.tag == "embeddedobject":
                try:
                    data = bytes.fromhex(value)
                except ValueError as e:
                    raise ValueError("Invalid embedded picture hexadecimal data") from e
            elif kind == "CDXCurvePoints":
                points = value.split()
                if len(points) % 2:
                    raise ValueError("Invalid curve points")
                data = packed("H", len(points) // 2) + b"".join(
                    coordinates(" ".join(points[i : i + 2]), "CDXPoint2D")
                    for i in range(0, len(points), 2)
                )
            elif kind == "INT16ListWithCounts":
                vals = [int(v) for v in value.split()]
                data = packed("H", len(vals)) + b"".join(packed("H", v) for v in vals)
            else:
                raise ValueError("Unsupported binary property type: " + name)
            result += property_bytes(code, data)
        if el.tag == "t":
            result += property_bytes(0x700, encode_text(el))
        for child in el:
            if child.tag == "s":
                continue
            if child.tag == "fonttable":
                # Explicit UTF-8 charsets make all outgoing style runs portable.
                data = packed("HH", 0, len(child))
                for font in child:
                    name = font.get("name", "Arial").encode("utf-8")
                    data += packed("HHH", int(font.attrib["id"]), 65001, len(name)) + name
                result += property_bytes(0x100, data)
            elif child.tag == "colortable":
                data = packed("H", len(child))
                for color in child:
                    values = [float(color.get(axis, "0")) for axis in ("r", "g", "b")]
                    if any(not math.isfinite(v) or not 0 <= v <= 1 for v in values):
                        raise ValueError("Invalid drawing color")
                    data += packed("HHH", *(round(v * 65535) for v in values))
                result += property_bytes(0x300, data)
            elif child.tag == "represent":
                attribute = BY_NAME.get(child.get("attribute", ""))
                if attribute is None:
                    raise ValueError("Unsupported represented property")
                result += property_bytes(
                    0xE, packed("IH", int(child.attrib["object"]), attribute[0])
                )
            else:
                result += write(child, depth + 1)
        result += b"\0\0"
        if len(result) > LIMIT:
            raise ValueError("Binary drawing is too large")
        return bytes(result)

    # Native clipboard uses the SDK's 22-byte header followed by the document.
    return b"VjCD0100\x04\x03\x02\x01" + bytes(10) + write(root) + b"\0\0"


def from_cdx(data):
    if len(data) > LIMIT:
        raise ValueError("Drawing exceeds the 16 MB structure limit")
    if not data.startswith(b"VjCD0100\x04\x03\x02\x01"):
        raise ValueError("Invalid binary drawing header")
    reader = Reader(data)
    # Some exporters follow the 28-byte header in the prose specification.
    if data[22:24] == b"\0\x80":
        reader.take(22)
    elif data[28:30] == b"\0\x80":
        reader.take(28)
    else:
        raise ValueError("Binary drawing has no document object")
    count, property_count, fonts, identifiers = 0, 0, {}, set()

    def read_object(code, depth=0):
        nonlocal count, property_count
        count += 1
        if depth > 64 or count > MAX_OBJECTS:
            raise ValueError("Drawing object limit exceeded")
        if code not in OBJECTS:
            raise ValueError(f"Unsupported binary drawing object 0x{code:04x}")
        el = ET.Element(OBJECTS[code])
        identifier = reader.number("I")
        if identifier and identifier in identifiers:
            raise ValueError("Duplicate binary drawing object identifier")
        identifiers.add(identifier)
        if identifier:
            el.set("id", str(identifier))
        raw, children = [], []
        while True:
            tag = reader.number("H")
            if tag == 0:
                break
            if tag >= 0x8000:
                children.append(read_object(tag, depth + 1))
                continue
            property_count += 1
            if property_count > 1_000_000:
                raise ValueError("Drawing property limit exceeded")
            size = reader.number("H")
            if size == 65535:
                size = reader.number("I")
            raw.append((tag, reader.take(size)))
        return el, raw, children

    tree = read_object(reader.number("H"))
    if tree[0].tag != "CDXML":
        raise ValueError("Expected a binary document")
    if reader.take(len(data) - reader.position) not in (b"", b"\0\0"):
        raise ValueError("Unexpected trailing binary drawing data")

    def convert(tree):
        el, raw, children = tree
        texts = {}
        # Font tables precede text decoding regardless of their file ordering.
        for tag, data in raw:
            if tag == 0x100:
                r = Reader(data)
                r.number("H")
                table = ET.SubElement(el, "fonttable")
                for _ in range(r.number("H")):
                    identifier, charset, length = r.number("H"), r.number("H"), r.number("H")
                    encoding = ENCODINGS.get(charset)
                    if encoding is None:
                        raise ValueError(f"Unsupported drawing text charset {charset}")
                    name = r.take(length).decode("utf-8" if charset == 65001 else "mac_roman")
                    fonts[identifier] = encoding
                    ET.SubElement(table, "font", id=str(identifier), name=name, charset=encoding)
                r.done()
        for tag, data in raw:
            if tag == 0x100:
                continue
            prop = PROPERTIES.get(tag)
            if prop is None:
                if el.tag in ("CDXML", "page") or (el.tag == "n" and tag in NODE_BOOKKEEPING):
                    continue
                raise ValueError(f"Unsupported binary drawing property 0x{tag:04x}")
            name, kind, enum = prop
            if tag in (0x700, 0x709):
                texts[tag] = data
                continue
            if kind in INTS:
                el.set(name, numeric(name, kind, enum, data, False))
            elif kind in ("CDXBoolean", "CDXBooleanImplied"):
                if data not in (b"", b"\0", b"\1"):
                    raise ValueError("Invalid drawing boolean")
                el.set(name, "no" if data == b"\0" else "yes")
            elif kind in ("CDXPoint2D", "CDXPoint3D", "CDXRectangle"):
                el.set(name, coordinates(data, kind, False))
            elif kind == "CDXString":
                temporary = ET.Element("t")
                decode_text(data, temporary, fonts)
                el.set(name, "".join(s.text or "" for s in temporary))
            elif kind == "CDXColorTable":
                r = Reader(data)
                table = ET.SubElement(el, "colortable")
                for _ in range(r.number("H")):
                    ET.SubElement(
                        table,
                        "color",
                        {axis: str(r.number("H") / 65535) for axis in ("r", "g", "b")},
                    )
                r.done()
            elif kind == "CDXFontStyle":
                if len(data) != 8:
                    raise ValueError("Invalid default font style")
                font, face, size, color = struct.unpack("<HHHH", data)
                prefix = "Label" if tag == 0x80A else "Caption"
                for suffix, value in [
                    ("Font", font),
                    ("Face", face),
                    ("Size", size / 20),
                    ("Color", color),
                ]:
                    el.set(prefix + suffix, str(value))
            elif kind == "CDXCurvePoints":
                r = Reader(data)
                count = r.number("H")
                el.set(
                    name,
                    " ".join(coordinates(r.take(8), "CDXPoint2D", False) for _ in range(count)),
                )
                r.done()
            elif kind in ("CDXObjectIDArray", "INT16ListWithCounts"):
                r = Reader(data)
                count = len(data) // 4 if kind == "CDXObjectIDArray" else r.number("H")
                el.set(
                    name,
                    " ".join(
                        str(r.number("I" if kind == "CDXObjectIDArray" else "H"))
                        for _ in range(count)
                    ),
                )
                r.done()
            elif kind == "CDXRepresentsProperty":
                r = Reader(data)
                identifier, attribute = r.number("I"), r.number("H")
                r.done()
                prop = PROPERTIES.get(attribute)
                if prop is None:
                    raise ValueError("Unsupported represented property")
                ET.SubElement(el, "represent", object=str(identifier), attribute=prop[0])
            elif kind == "varies" and name == "Value":
                if data:
                    raise ValueError("Unsupported binary object-tag value")
            elif kind == "Unformatted" and el.tag == "embeddedobject":
                el.set(name, data.hex())
            elif el.tag in ("CDXML", "page") and kind in ("Unformatted", "CDXDate"):
                continue
            else:
                raise ValueError("Unsupported binary drawing property: " + name)
        if texts:
            tag = 0x709 if 0x709 in texts else 0x700
            decode_text(texts[tag], el, fonts, tag == 0x709)
        for child in children:
            el.append(convert(child))
        return el

    return ET.tostring(convert(tree), encoding="unicode")
