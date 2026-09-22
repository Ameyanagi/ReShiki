"""Direct original arrow reader, including worker coordinate conversion."""

import itertools
import json
import math
import random
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.arrows_exchange import read_arrow

COLORS = [[0, 0, 0], [255, 255, 255], [255, 255, 255], [0, 0, 0], [20, 40, 80]]
QUADRATIC = [(0, 0), (0, 0), (10, 20 / 3), (20, 20 / 3), (30, 0), (30, 0)]
ELBOW = [(0, 0), (0, 0), (10, 0), (20, 0), (30, 0), (30, 10), (30, 20), (30, 30), (30, 30)]


def native(value):
    if isinstance(value, float) and not math.isfinite(value):
        return "NaN" if math.isnan(value) else ("Infinity" if value > 0 else "-Infinity")
    if isinstance(value, dict):
        return {key: native(item) for key, item in value.items()}
    if isinstance(value, list):
        return [native(item) for item in value]
    return value


def emit(
    name,
    attrs=None,
    tag="arrow",
    root_attrs=None,
    colors=None,
    scale=1,
    points=None,
    identifier=10,
    prefix="",
    restriction=None,
):
    root = ET.Element("CDXML", root_attrs or {})
    page = ET.SubElement(root, "page")
    group = ET.SubElement(page, "group", id="duplicate")
    values = dict(id="duplicate", ArrowheadHead="Full", Tail3D="0 0 0", Head3D="30 0 0")
    if tag == "curve":
        values["CurvePoints"] = " ".join(
            format(v, ".17g") for p in (points if points is not None else QUADRATIC) for v in p
        )
    values.update(attrs or {})
    el = ET.SubElement(group, tag, {k: v for k, v in values.items() if v is not None})
    text = prefix + ET.tostring(root, encoding="unicode")
    colors = colors if colors is not None else COLORS
    expected = failure = failure_type = None
    calls = []

    def point(text):
        calls.append(text)
        values = [float(v) for v in text.split()]
        if len(values) < 2 or not all(math.isfinite(v) for v in values):
            raise ValueError("Invalid CDXML coordinates")
        return dict(x=values[0] * scale, y=values[1] * scale)

    try:
        root = ET.fromstring(text)
        el = list(root.iter())[3]
        before = ET.tostring(root)
        expected = read_arrow(el, root, point, colors, identifier)
        assert before == ET.tostring(root)
    except (ValueError, KeyError, OverflowError, ET.ParseError) as exc:
        failure, failure_type = str(exc), type(exc).__name__
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                source=3,
                colors=colors,
                scale=scale,
                identifier=identifier,
                expected=native(expected),
                failure=failure,
                failure_type=failure_type,
                calls=calls,
                restriction=restriction,
            )
        )
    )


def main():
    for tag, head, tail, shape, line, gap in itertools.product(
        ("arrow", "curve"),
        ("None", "Full", "HalfLeft", "HalfRight"),
        ("None", "Full", "HalfLeft", "HalfRight"),
        ("Solid", "Hollow", "Angle"),
        ("Solid", "Dashed", "Bold"),
        ("0", "100"),
    ):
        emit(
            f"appearance-{tag}-{head}-{tail}-{shape}-{line}-{gap}",
            dict(
                ArrowheadHead=head,
                ArrowheadTail=tail,
                ArrowheadType=shape,
                LineType=line,
                ArrowShaftSpacing=gap,
            ),
            tag=tag,
        )
    for key, values in {
        "AngularSize": ("0", "-0", "1", "nan", "inf", "bad", "０"),
        "FillType": ("None", "Unspecified", "Solid", ""),
        "FadePercent": ("100", "1_00", "１００", "99", "nan", "bad"),
        "LineType": ("solid", "Dotted", ""),
        "ArrowheadHead": (None, "Unspecified", "Half", "bad"),
        "ArrowheadTail": (None, "Unspecified", "bad"),
        "ArrowheadType": (None, "Unspecified", "bad"),
        "NoGo": ("None", "Cross", "HASH", "cRoSs", "bad", ""),
        "Dipole": (None, "yes", "no", "YES", "bad"),
        "color": ("0", "1", "4", "5", "-1", "４", "0_4", "4.0", "bad", "9" * 100),
        "HeadSize": (
            "0",
            "-1",
            "49.999999",
            "50",
            "2400",
            "2400.0001",
            "nan",
            "inf",
            "bad",
            "1e309",
        ),
        "ArrowheadWidth": ("0", "24.999999", "25", "1600", "1600.00001", "nan", "inf", "bad"),
        "ArrowheadCenterSize": (
            None,
            "0",
            "99.99999",
            "100",
            "1000",
            "1000.00001",
            "-1",
            "nan",
            "inf",
            "bad",
        ),
        "LineWidth": ("0", ".09999999", ".1", "12", "12.00001", "nan", "inf", "bad"),
        "ArrowShaftSpacing": (
            "0",
            "-0",
            "-100",
            "49.99999",
            "50",
            "1600",
            "1600.00001",
            "nan",
            "inf",
            "bad",
        ),
        "Tail3D": (
            None,
            "",
            "0",
            "1 2",
            "1 2 3 4",
            "nan 0 0",
            "0 0 nan",
            "0 0 inf",
            "bad 0",
            "１ ٢ ٣",
            "1_0 2 0",
        ),
        "Head3D": (None, "", "0", "bad 0", "1 2 3 4"),
    }.items():
        for value in values:
            emit(f"attribute-{key}-{value}", {key: value})
    for tag, line, value in itertools.product(
        ("arrow", "curve"),
        ("Solid", "Bold"),
        (
            "0",
            "2",
            "4",
            "126",
            "127",
            "-1",
            "-4",
            "bad",
            str(2**200 + 4),
            str(-(2**200 + 4)),
            "9" * 4301,
        ),
    ):
        emit(f"flags-{tag}-{line}-{value[:30]}", dict(LineType=line, CurveType=value), tag=tag)
    for root_key, own, inherited in itertools.product(
        ("LineWidth", "BoldWidth", "color"), (None, "0.6", "bad"), ("1", "bad")
    ):
        emit(
            f"inherit-{root_key}-{own}-{inherited}",
            {root_key: own, "LineType": "Bold" if root_key == "BoldWidth" else "Solid"},
            root_attrs={root_key: inherited},
        )
    for color in (-1, 255.5, 256, 1e100, 100):
        emit(f"palette-{color}", colors=[[color, 0, 0]] * 4)
    for tag, closed in itertools.product(("arrow", "curve"), (None, "no", "yes", "bad")):
        emit(f"closed-{tag}-{closed}", dict(Closed=closed), tag=tag)
    for count in (0, 1, 5, 11, 13, 17, 19, 20):
        emit(
            f"point-count-{count}",
            dict(CurvePoints=" ".join("0" for _ in range(count))),
            tag="curve",
        )
    for i in range(18):
        points = [format(v, ".17g") for point in ELBOW for v in point]
        points[i] = "nan" if i % 2 else "bad"
        emit(f"bad-elbow-coordinate-{i}", dict(CurvePoints=" ".join(points)), tag="curve")
    emit("elbow", tag="curve", points=ELBOW)
    for distance in (math.nextafter(0.15, -math.inf), 0.15, math.nextafter(0.15, math.inf), -0.15):
        p = list(ELBOW)
        p[2] = (10, distance)
        emit(f"elbow-cross-{distance!r}", tag="curve", points=p)
    for length in (
        math.nextafter(1e-6, 0),
        1e-6,
        math.nextafter(1e-6, math.inf),
        1e-308,
        5e-324,
        0,
    ):
        p = [
            (0, 0),
            (0, 0),
            (-0.01, 0),
            (0, 0),
            (length, 0),
            (length, 1),
            (length, 2),
            (length, 3),
            (length, 3),
        ]
        emit(f"degenerate-segment-{length!r}", tag="curve", points=p)
    rng = random.Random(317)
    for i in range(500):
        angle = rng.uniform(-math.pi, math.pi)
        x, y = 0.15 * math.cos(angle), 0.15 * math.sin(angle)
        for direction in (-math.inf, None, math.inf):
            near = math.nextafter(x, direction) if direction is not None else x
            p = [(0, 0), (0, 0), (near / 1.5, y / 1.5), (0, 0), (0, 0), (0, 0)]
            emit(f"quadratic-threshold-{i}-{direction}", tag="curve", points=p)
    for i in range(300):
        start, control, end = [
            (rng.uniform(-1000, 1000), rng.uniform(-1000, 1000)) for _ in range(3)
        ]
        a = tuple(start[k] + 2 * (control[k] - start[k]) / 3 for k in (0, 1))
        b = tuple(end[k] + 2 * (control[k] - end[k]) / 3 for k in (0, 1))
        emit(
            f"random-quadratic-{i}",
            tag="curve",
            points=[start, start, a, b, end, end],
            scale=rng.choice((1, 0.5, -2, 42 / 14.4)),
        )
    for i in range(250):
        bits = rng.randrange(0x3F000000, 0x461C0000)
        lo = struct.unpack("!f", struct.pack("!I", bits))[0]
        hi = struct.unpack("!f", struct.pack("!I", bits + 1))[0]
        center = (lo + hi) / 2
        for toward in (-math.inf, None, math.inf):
            x = math.nextafter(center, toward) if toward is not None else center
            emit(f"f32-midpoint-{i}-{toward}", dict(Tail3D=f"{x:.17g} {-x:.17g} 0"))
    emit("large-straight", dict(Head3D="1e300 0 0"))
    emit("overflow-scaled-point", dict(Head3D="1e308 0 0"), scale=2)
    emit("overflow-quadratic", tag="curve", points=[(1e308, 1e308)] * 6, scale=2)
    emit("external-dtd", prefix='<!DOCTYPE CDXML SYSTEM "https://example.invalid/cdxml.dtd">')
    emit(
        "dtd-restriction",
        prefix='<!DOCTYPE CDXML [<!ATTLIST arrow Dipole CDATA "yes">]>',
        restriction="Internal DTD",
    )
    emit(
        "namespace-restriction",
        root_attrs={"xmlns:x": "urn:x", "x:test": "1"},
        restriction="Namespace",
    )


if __name__ == "__main__":
    main()
