"""Independent direct-original graphics reader oracle, including deferred images."""

import base64
import copy
import io
import itertools
import json
import math
import random
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

from PIL import Image
from rdkit import rdBase

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine import graphics_exchange

ROOT = Path(__file__).resolve().parents[1]
POINTS = "0 0 1 2 3 4 5 6 7 8 9 10"


def xml(tag="graphic", attrs=None, root_attrs=None, chemistry=None):
    root = ET.Element("CDXML", root_attrs or {})
    page = ET.SubElement(root, "page", id="page")
    if chemistry is not None:
        ET.SubElement(page, "n", Z=str(chemistry))
    ET.SubElement(
        page,
        tag,
        {
            **(
                {"GraphicType": "Rectangle", "BoundingBox": "1 2 9 12"}
                if tag == "graphic"
                else {"CurvePoints": POINTS}
            ),
            **(attrs or {}),
        },
    )
    return root


def safe(value):
    if isinstance(value, float) and not math.isfinite(value):
        return None
    if isinstance(value, dict):
        return {k: safe(v) for k, v in value.items()}
    if isinstance(value, list):
        return [safe(v) for v in value]
    return value


def emit(
    name,
    root,
    scale=42 / 14.4,
    first_id=100,
    claimed=(),
    source=None,
    images=False,
    pixel_reference=True,
):
    root = copy.deepcopy(root)
    before = ET.tostring(root, encoding="unicode")
    elements = list(root.iter())
    objects = {elements[i]: [1000 + i] for i in claimed}
    snapshot = copy.deepcopy(list(objects.values()))
    try:
        expected = graphics_exchange.read_graphics(
            root, scale, first_id, objects, local_pictures=True
        )
        error = error_type = None
    except (ValueError, KeyError, OverflowError) as exc:
        expected, error, error_type = None, str(exc), type(exc).__name__
    assert ET.tostring(root, encoding="unicode") == before
    assert [objects[elements[i]] for i in claimed] == snapshot
    bindings = [
        [i, objects[el][0]] for i, el in enumerate(elements) if el in objects and i not in claimed
    ]
    # Output traversal order differs from element order only when a parent merges.
    bindings.sort(key=lambda pair: pair[1])
    document = copy.deepcopy(expected)
    image_pixels = []
    image_error = None
    if images and expected is not None:
        try:
            direct = graphics_exchange.read_graphics(root, scale, first_id, local_pictures=False)
            for graphic in direct:
                if "picture" in graphic:
                    decoded = Image.open(io.BytesIO(base64.b64decode(graphic["picture"]))).convert(
                        "RGBA"
                    )
                    image_pixels.append(
                        dict(
                            width=decoded.width,
                            height=decoded.height,
                            pixels=base64.b64encode(decoded.tobytes()).decode(),
                        )
                    )
        except ValueError as exc:
            image_error = str(exc)
    layer_boundary = expected is not None and any(
        not -(2**31) <= g["layer"] < 2**31 for g in expected
    )
    id_boundary = expected is not None and any(not 0 <= g["id"] < 2**64 for g in expected)
    if expected is not None:
        for graphic in expected:
            graphic["layer"] = str(graphic["layer"])
        if layer_boundary or id_boundary:
            document = None
    print(
        json.dumps(
            dict(
                name=name,
                xml=before if source is None else source,
                scale=scale,
                first_id=first_id,
                claimed=list(claimed),
                expected=safe(expected),
                document=safe(document),
                bindings=bindings,
                error=error,
                error_type=error_type,
                layer_boundary=layer_boundary,
                id_boundary=id_boundary,
                images=images,
                pixel_reference=pixel_reference,
                image_error=image_error,
                image_pixels=image_pixels,
            )
        )
    )


def raster(fmt="PNG", subsampling=0):
    image = Image.new("RGBA", (3, 2))
    image.putdata(
        [
            (255, 0, 0, 255),
            (0, 255, 0, 128),
            (0, 0, 255, 0),
            (1, 2, 3, 17),
            (255, 255, 0, 192),
            (127, 23, 222, 234),
        ]
    )
    stream = io.BytesIO()
    (image.convert("RGB") if fmt == "JPEG" else image).save(
        stream, format=fmt, **({"subsampling": subsampling} if fmt == "JPEG" else {})
    )
    return stream.getvalue().hex()


def main():
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    emit("empty", ET.Element("CDXML"))
    for kind in [
        None,
        "",
        "Rectangle",
        "Line",
        "Oval",
        "Bracket",
        "Symbol",
        "Orbital",
        "Arc",
        "Unknown",
    ]:
        root = xml(attrs={"GraphicType": kind or ""})
        if kind is None:
            del root.find("page/graphic").attrib["GraphicType"]
        emit(f"kind/{kind}", root)
    for flags in itertools.product(["Plain", "RoundEdge", "Filled", "Dashed", "Bold"], repeat=3):
        emit(f"rectangle/{flags}", xml(attrs={"RectangleType": " ".join(flags)}))
    for flags in [
        "",
        "Shaded",
        "Shadow",
        "RoundEdge Filled Dashed Bold Plain",
        "Plain\u2003Bold",
        "plain",
    ]:
        emit(f"rectangle/{flags}", xml(attrs={"RectangleType": flags}))
    for kind in ["SquarePair", "RoundPair", "CurlyPair", "Square", "", "RoundPair "]:
        emit(f"bracket/{kind}", xml(attrs={"GraphicType": "Bracket", "BracketType": kind}))
    for kind in [
        "CirclePlus",
        "CircleMinus",
        "Plus",
        "Minus",
        "Radical",
        "Electron",
        "LonePair",
        "ElectronPair",
        "RadicalCation",
        "RadicalAnion",
        "HDot",
        "HDash",
        "Attachment",
        "Star",
        "",
    ]:
        for box in ["1 2 9 12", "9 12 1 2", "0 0 0 0", "0 0 0.0003 0.0002", "-0 0 0.001 0"]:
            emit(
                f"symbol/{kind}/{box}",
                xml(attrs={"GraphicType": "Symbol", "SymbolType": kind, "BoundingBox": box}),
                scale=1,
            )
    for key, value in itertools.product(
        ["ChemicallySignificant", "ObjectID"], ["", "yes", "no", "0"]
    ):
        emit(
            f"attached/{key}/{value}",
            xml(attrs={"GraphicType": "Symbol", "SymbolType": "Plus", key: value}),
        )
    for name, suffix in itertools.product(
        [
            "s",
            "oval",
            "sigma",
            "lobe",
            "sp",
            "p",
            "sp3",
            "dxy",
            "dz2",
            "sSolidSolid",
            "dz2unknown",
            "",
        ],
        ["", "2", "Solid", "Solid2", "Shaded", "Shaded2"],
    ):
        emit(
            f"orbital/{name}/{suffix}",
            xml(attrs={"GraphicType": "Orbital", "OrbitalType": name + suffix}),
        )
    oval = {
        "GraphicType": "Oval",
        "Center3D": "12 25 0",
        "MajorAxisEnd3D": "30 10 7",
        "MinorAxisEnd3D": "7 20 -8",
    }
    for flags in itertools.product(["Plain", "Circle", "Filled", "Dashed", "Bold"], repeat=2):
        emit(f"oval/{flags}", xml(attrs={**oval, "OvalType": " ".join(flags)}))
    for key in ["Center3D", "MajorAxisEnd3D", "MinorAxisEnd3D"]:
        for value in ["0", "1 2", "1 2 3 4", "1 2 nan", "1 2 bad", "", "１ ٢"]:
            emit(f"ovalpoint/{key}/{value}", xml(attrs={**oval, key: value}))
    for key, values in {
        "LineType": ["Solid", "Dashed", "Bold", "Wavy", "Double", ""],
        "BoundingBox": [
            "",
            "1 2",
            "1 2 3 4 5",
            "1 2 nan 4",
            "1 2 bad 4",
            "-0 -0 0 0",
            "1e308 0 -1e308 4",
            "1 2 1e-300 4",
        ],
        "ArrowType": ["NoHead", "", "FullHead"],
        "BracketUsage": ["Unspecified", "", "SRU"],
        "color": ["0", "1", "2", "3", "4", "-1", "0x3", "3_0", " ٣ ", "bad", "9" * 100, "9" * 4301],
        "LineWidth": [
            "0",
            "-0",
            "0.1",
            "12",
            "-0.01",
            "12.0000001",
            "nan",
            "inf",
            "bad",
            "1_0",
            "１.２",
        ],
        "BoldWidth": ["0", "-1", "12", "13", "nan", "bad"],
        "FadePercent": ["100", "100.0000001", "99", "nan", "inf", "bad"],
        "alpha": ["1", "0", "0.9999999", "nan", "bad"],
        "Z": [
            "0",
            "-1",
            "32767",
            "32768",
            "-2147483648",
            "2147483647",
            "9" * 100,
            "-" + "9" * 100,
            " ١٢_٣ ",
            "bad",
            "9" * 4301,
        ],
    }.items():
        for value in values:
            for inherited in [False, True]:
                attrs = {
                    "GraphicType": "Line",
                    "LineType": "Bold" if key == "BoldWidth" else "Solid",
                }
                if not inherited:
                    attrs[key] = value
                emit(
                    f"attribute/{key}/{value[:30]}/{inherited}",
                    xml(attrs=attrs, root_attrs={key: value} if inherited else {}),
                )
    for key in ["CurveType", "Closed", "FillType", "LineType", "ArrowheadHead", "ArrowheadTail"]:
        values = (
            list(range(-4, 262))
            if key == "CurveType"
            else ["None", "Unspecified", "Solid", "Dashed", "Bold", "yes", "no", "", "Full", "bad"]
        )
        for value in values:
            emit(f"curve/{key}/{value}", xml("curve", {key: str(value)}))
    for count in range(0, 43):
        emit(
            f"curvepoints/{count}",
            xml("curve", {"CurvePoints": " ".join(str(n) for n in range(count))}),
        )
    for points in [
        POINTS.replace("7", "nan"),
        POINTS.replace("7", "bad"),
        "0 0 1 2 3 4 5 6 1 2 9 10",
        "0 0 1 2 3 4 5 6 1.0000000000000002 2 9 10",
    ]:
        for closed in ["yes", "no"]:
            emit(
                f"closed/{points}/{closed}", xml("curve", {"CurvePoints": points, "Closed": closed})
            )
    missing = xml("curve")
    del missing.find("page/curve").attrib["CurvePoints"]
    emit("missing-curvepoints", missing)
    # Layer differences must remain exact even far beyond machine integer range.
    for chemistry, graphic in itertools.product(
        [0, 32767, -32768, -(2**80), 2**80, 10**300], repeat=2
    ):
        emit(f"layer/{chemistry}/{graphic}", xml(attrs={"Z": str(graphic)}, chemistry=chemistry))
    for delta in [-2147483649, -2147483648, -1, 0, 1, 2147483646, 2147483647]:
        emit(
            f"huge-cancellation/{delta}", xml(attrs={"Z": str(10**300 + delta)}, chemistry=10**300)
        )
    # Chemistry Z is considered anywhere in root, including objects not collected.
    for tag in ["fragment", "n", "b", "t", "arrow", "curve", "page", "group"]:
        root = xml()
        ET.SubElement(root, tag, Z="bad")
        emit(f"middle/{tag}", root)
    # The exact tree and object membership, not string IDs, control collection.
    for matching, filled, second_fill, count in itertools.product(
        [False, True], [False, True], [False, True], [0, 1, 2, 3]
    ):
        root = ET.Element("CDXML")
        page = ET.SubElement(root, "page")
        group = ET.SubElement(page, "group", id="same", Z="57")
        for i in range(count):
            ET.SubElement(
                group,
                "curve",
                id="same",
                CurvePoints=POINTS if matching or i == 0 else POINTS.replace("7", "8"),
                FillType="Solid" if (filled if i == 0 else second_fill) else "None",
                Z=str(i),
                color=str(i % 4),
            )
        for claim in [(), (2,), *(((3,),) if count else ())]:
            emit(f"group/{matching}/{filled}/{second_fill}/{count}/{claim}", root, claimed=claim)
    for tag in ["group", "fragment", "n", "page", "unknown"]:
        root = ET.Element("CDXML")
        page = ET.SubElement(root, "page")
        container = ET.SubElement(page, tag, id="duplicate")
        ET.SubElement(
            container, "graphic", GraphicType="Rectangle", BoundingBox="1 2 3 4", id="duplicate"
        )
        for claim in [(), (2,), (3,)]:
            emit(f"descent/{tag}/{claim}", root, claimed=claim)
    for value in ["", "0", "false", "100"]:
        root = xml(root_attrs={"SupersededBy": "1"})
        root.find("page/graphic").set("SupersededBy", value)
        emit(f"superseded/{value}", root)
    for first_id in [0, 2**53 + 1, 2**64 - 2, 2**64 - 1]:
        root = xml()
        root.find("page").append(copy.deepcopy(root.find("page/graphic")))
        emit(f"ids/{first_id}", root, first_id=first_id)
    for color in ["-.1", "1.1", ".5", "1e300", "nan", "inf", "bad"]:
        for used in [False, True]:
            root = xml(attrs={"color": "2" if used else "0"})
            table = ET.SubElement(root, "colortable")
            ET.SubElement(table, "color", r=color, g=".5", b="0")
            emit(f"palette/{color}/{used}", root)
    rng = random.Random(19981)
    # Exact f64 geometry, with f32 midpoint and hypot decision boundaries.
    for i in range(1000):
        theta = rng.uniform(-math.pi, math.pi)
        distance = math.nextafter(0.001, [0, math.inf][i % 2]) if i % 3 else 0.001
        x, y = math.cos(theta) * distance, math.sin(theta) * distance
        emit(
            f"hypot/{i}",
            xml(
                attrs={"GraphicType": "Symbol", "SymbolType": "Plus", "BoundingBox": f"0 0 {x} {y}"}
            ),
            scale=1,
        )
    for i in range(500):
        values = [rng.uniform(-1e5, 1e5) for _ in range(4)]
        scale = rng.uniform(-100, 100)
        emit(
            f"random-frame/{i}",
            xml(attrs={"GraphicType": "Line", "BoundingBox": " ".join(map(str, values))}),
            scale=scale,
        )
    for i in range(300):
        center = rng.randrange(-(10**100), 10**100)
        offset = rng.randrange(-(10**20), 10**20) if i % 2 else rng.randrange(-(10**8), 10**8)
        emit(f"large-layer-random/{i}", xml(attrs={"Z": str(center + offset)}, chemistry=center))
    for bits in [1, 0x3DCCCCCD, 0x3F800000, 0x3F800001, 0x42C80000, 0x7F7FFFFE]:
        a, b = [struct.unpack("<f", struct.pack("<I", n))[0] for n in [bits, bits + 1]]
        midpoint = (a + b) / 2
        for value in [
            math.nextafter(midpoint, -math.inf),
            midpoint,
            math.nextafter(midpoint, math.inf),
        ]:
            emit(
                f"f32-midpoint/{bits}/{value}",
                xml(attrs={"GraphicType": "Line", "BoundingBox": f"0 0 {value} {-value}"}),
                scale=1,
            )
    # Multiple pages, namespace-disqualified objects, comments in merge groups,
    # and SupersededBy within an otherwise mergeable group preserve identity.
    root = ET.fromstring(
        '<CDXML><page><graphic GraphicType="Line" BoundingBox="0 0 1 2" id="x"/></page><unknown><page><graphic/></page></unknown><page><graphic xmlns="urn:other"/><graphic GraphicType="Line" BoundingBox="4 5 6 7" id="x"/></page></CDXML>'
    )
    emit("page-and-namespace-order", root)
    root = ET.Element("CDXML")
    page = ET.SubElement(root, "page")
    group = ET.SubElement(page, "group", Z="3")
    ET.SubElement(group, "curve", CurvePoints=POINTS, FillType="Solid", SupersededBy="x")
    ET.SubElement(group, "curve", CurvePoints=POINTS, LineType="Dashed")
    emit("merged-superseded-child", root)
    for index in [0, 1]:
        altered = copy.deepcopy(root)
        altered.find("page/group")[index].set("LineWidth", "bad")
        emit(f"merge-eager-style-error/{index}", altered)
    # Numeric errors are evaluated even in extra 3D components, but unsupported
    # shape flags precede style parsing; unused root alpha/fade are ignored.
    for attrs in [
        {"LineType": "bad", "color": "bad"},
        {"BoundingBox": "bad", "LineType": "bad"},
        {"FadePercent": "0", "alpha": "bad"},
    ]:
        emit(f"error-order/{attrs}", xml(attrs={"GraphicType": "Line", **attrs}))
    for value in ["bad", "9" * 4300]:
        emit(
            f"unused-root-alpha/{value[:10]}",
            xml(root_attrs={"alpha": value, "FadePercent": value}),
        )
    for scale in [0.0, -0.0, -3, 1e308, 1e-308]:
        emit(f"scale/{scale}", xml(), scale=scale)
    for fmt, angle, opacity in itertools.product(
        ["PNG", "TIFF", "JPEG", "GIF", "BMP"], [0, 30, 90, -57.315, 359.999999, 1e20], [0, 0.5, 1]
    ):
        emit(
            f"picture/{fmt}/{angle}/{opacity}",
            xml(
                "embeddedobject",
                {
                    fmt: raster(fmt),
                    "BoundingBox": "10 20 82 56",
                    "RotationAngle": str(angle * 65536),
                    "alpha": str(opacity),
                },
            ),
            images=True,
        )
    # Existing JPEG chroma upsampling differs from Pillow for this tiny image.
    # Keep the exact current deferred-image bridge contract without claiming
    # Pillow pixel parity; the standalone image/array reproducer tracks it.
    emit(
        "picture/JPEG-subsampled-existing-decoder",
        xml("embeddedobject", {"JPEG": raster("JPEG", 2), "BoundingBox": "0 0 10 20"}),
        images=True,
        pixel_reference=False,
    )
    for key, values in {
        "PNG": [
            "",
            "00",
            "not-hex",
            "0 0",
            "  00\nff\t",
            "０１",
            "00\u00a000",
            "00\vff",
            "00\x1cff",
        ],
        "alpha": ["-0.1", "1.1", "nan", "inf", "bad"],
        "BoundingBox": [
            "",
            "1 2 3",
            "1 2 3 4 5",
            "1 2 1 4",
            "1 2 nan 4",
            "1 2 bad 4",
            "4 5 1 2",
            "0 0 .003428571428571429 2",
        ],
        "RotationAngle": ["nan", "inf", "bad", "1e308"],
    }.items():
        for value in values:
            # XML 1.0 cannot carry vertical-tab/control separators; encode oracle
            # scalar behavior separately only through XML-valid whitespace.
            if any(ord(ch) < 32 and ch not in "\n\t\r" for ch in value):
                continue
            emit(
                f"picture-invalid/{key}/{value}",
                xml("embeddedobject", {"PNG": raster(), "BoundingBox": "0 0 10 20", key: value}),
                images=True,
            )
    emit(
        "picture-preference",
        xml("embeddedobject", {"PNG": raster(), "TIFF": "bad", "BoundingBox": "0 0 10 20"}),
        images=True,
    )
    emit(
        "picture-fallback",
        xml("embeddedobject", {"PNG": "", "TIFF": raster("TIFF"), "BoundingBox": "0 0 10 20"}),
        images=True,
    )
    for name in ["graphics-chemdraw.cdxml", "native-export.cdxml"]:
        path = ROOT / "tests/fixtures" / name
        if path.exists():
            emit(f"fixture/{name}", ET.fromstring(path.read_text()))
    simple = ET.tostring(xml(), encoding="unicode")
    for name, source in [
        ("comment", "<!-- <!ENTITY inert 'x'> -->" + simple),
        ("cdata", simple.replace("</page>", "<![CDATA[<!ENTITY inert 'x'>]]></page>")),
        ("pi", "<?example <!ENTITY inert 'x'> ?>" + simple),
    ]:
        emit(f"inert/{name}", ET.fromstring(source), source=source)


if __name__ == "__main__":
    main()
