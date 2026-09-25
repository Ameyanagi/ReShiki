"""CDXML vector-graphic exchange. Coordinates come from the editor's scene model."""

import math
import xml.etree.ElementTree as ET
from typing import TYPE_CHECKING

if TYPE_CHECKING or __package__:
    from .pictures_exchange import read_picture, write_picture
else:
    from pictures_exchange import read_picture, write_picture


def palette(root):
    return [[0, 0, 0], [255, 255, 255]] + (
        [
            [round(float(el.get(axis, "0")) * 255) for axis in ("r", "g", "b")]
            for el in root.findall("./colortable/color")
        ]
        or [[255, 255, 255], [0, 0, 0]]
    )


def read_graphics(root, scale, first_id, object_map: dict | None = None, *, local_pictures=False):
    objects = {} if object_map is None else object_map
    colors = palette(root)
    chemistry_z = [
        int(el.attrib["Z"])
        for el in root.iter()
        if el.tag in ("fragment", "n", "b", "t", "arrow") and "Z" in el.attrib
    ]
    middle = min(chemistry_z, default=32767)

    def point(value):
        values = [float(v) for v in value.split()]
        if len(values) < 2 or not all(math.isfinite(v) for v in values):
            raise ValueError("Invalid graphic coordinates")
        return {"x": values[0] * scale, "y": values[1] * scale}

    def style(el, filled=False, dashed=False, bold=False):
        color = int(el.get("color", root.get("color", "3")))
        if not 0 <= color < len(colors):
            raise ValueError("Invalid graphic color index")
        width = float(
            el.get(
                "BoldWidth" if bold else "LineWidth",
                root.get("BoldWidth" if bold else "LineWidth", "2" if bold else "0.6"),
            )
        )
        if not math.isfinite(width) or not 0 <= width <= 12:
            raise ValueError("Graphic line width is outside the supported range")
        if float(el.get("FadePercent", "100")) != 100 or float(el.get("alpha", "1")) != 1:
            raise ValueError("Transparent/faded CDXML graphics are not supported yet")
        return {
            "stroke": colors[color],
            "fill": colors[color] if filled else None,
            "width_pt": max(0.1, width),
            "pattern": "dashed" if dashed else "solid",
        }

    def base(el, kind, origin, x, y, paint, path=None, sides="both"):
        z = int(el.get("Z", "0"))
        return {
            "kind": kind,
            "origin": origin,
            "axis_x": x,
            "axis_y": y,
            "style": paint,
            "sides": sides,
            "layer": z - middle if z < middle else z - middle + 1,
            "path": path or [],
        }

    def curve(el):
        flags = int(el.get("CurveType", "0"))
        if (
            flags & ~(1 | 2 | 4 | 128)
            or el.get("FillType", "None") not in ("None", "Solid", "Unspecified")
            or el.get("LineType", "Solid") not in ("Solid", "Dashed", "Bold")
            or any(
                el.get(k, "None") not in ("None", "Unspecified")
                for k in ("ArrowheadHead", "ArrowheadTail")
            )
        ):
            raise ValueError("Arrowed, shaded or doubled CDXML curves are not supported yet")
        values = el.attrib["CurvePoints"].split()
        if len(values) < 12 or len(values) % 6:
            raise ValueError("Invalid CDXML Bézier control-point sequence")
        points = [point(" ".join(values[i : i + 2])) for i in range(0, len(values), 2)]
        path = [{"command": "move", "points": points[1]}]
        # Each anchor has an incoming handle, anchor, outgoing handle.
        for i in range(3, len(points), 3):
            path.append({"command": "cubic", "points": [points[i - 1], points[i], points[i + 1]]})
        closed = bool(flags & 1) or el.get("Closed") == "yes"
        if closed:
            if points[-2] != points[1]:
                path.append({"command": "cubic", "points": [points[-1], points[0], points[1]]})
            path.append({"command": "close"})
        paint = style(
            el,
            bool(flags & 128) or el.get("FillType") == "Solid",
            bool(flags & 2) or el.get("LineType") == "Dashed",
            bool(flags & 4) or el.get("LineType") == "Bold",
        )
        return base(el, "path", {"x": 0, "y": 0}, {"x": 1, "y": 0}, {"x": 0, "y": 1}, paint, path)

    def graphic(el):
        kind = el.get("GraphicType")
        values = el.get("BoundingBox", "").split()
        if len(values) != 4:
            raise ValueError("Graphic is missing its two defining points")
        a, b = point(" ".join(values[:2])), point(" ".join(values[2:]))
        origin = {"x": min(a["x"], b["x"]), "y": min(a["y"], b["y"])}
        x = {"x": abs(b["x"] - a["x"]), "y": 0}
        y = {"x": 0, "y": abs(b["y"] - a["y"])}
        if (
            el.get("ArrowType", "NoHead") != "NoHead"
            or el.get("BracketUsage", "Unspecified") != "Unspecified"
        ):
            raise ValueError(
                "Chemical brackets and legacy graphic arrows need a dedicated import path"
            )
        if kind == "Orbital":
            label = el.get("OrbitalType", "")
            names = {
                "s": "s",
                "oval": "sigma",
                "sigma": "sigma",
                "lobe": "lobe",
                "sp": "lobe",
                "p": "p",
                "sp3": "hybrid",
                "dxy": "dxy",
                "dz2": "dz2",
            }
            if "Shaded" in label:
                raise ValueError("Gradient-shaded CDXML orbitals are not supported yet")
            phase = "solid" if "Solid" in label else "open"
            key = label.replace("Shaded", "").replace("Solid", "").removesuffix("2")
            # dz2 is a name, not the phase-reversal suffix.
            if label.startswith("dz2"):
                key = "dz2"
            if key not in names:
                raise ValueError("Unsupported CDXML orbital")
            axis = {k: a[k] - b[k] for k in ("x", "y")}
            value = base(
                el, {"orbital": names[key]}, b, axis, {"x": -axis["y"], "y": axis["x"]}, style(el)
            )
            value.update(phase=phase, phase_flipped=label.endswith("2") and not label == "dz2")
            return value
        if kind == "Symbol":
            names = {
                "CirclePlus": "circle_plus",
                "CircleMinus": "circle_minus",
                "Plus": "plus",
                "Minus": "minus",
                "Radical": "radical",
                "Electron": "radical",
                "LonePair": "lone_pair",
                "ElectronPair": "lone_pair",
                "RadicalCation": "radical_cation",
                "RadicalAnion": "radical_anion",
                "HDot": "hydrogen_dot",
                "HDash": "hydrogen_dash",
                "Attachment": "diamond",
            }
            key = el.get("SymbolType")
            if key not in names:
                raise ValueError("Unsupported CDXML chemical symbol")
            if el.get("ChemicallySignificant") == "yes" or el.get("ObjectID"):
                raise ValueError(
                    "Attached CDXML symbols require a dedicated chemical attachment importer"
                )
            size = math.hypot(b["x"] - a["x"], b["y"] - a["y"])
            if size < 0.001:
                raise ValueError("Invalid CDXML symbol size")
            axis = {k: a[k] - b[k] for k in ("x", "y")}
            if key in ("Electron", "Radical", "LonePair", "ElectronPair"):
                axis = {k: v * 2 for k, v in axis.items()}
            return base(
                el, {"symbol": names[key]}, a, axis, {"x": -axis["y"], "y": axis["x"]}, style(el)
            )
        if kind == "Line":
            if el.get("LineType", "Solid") not in ("Solid", "Dashed", "Bold"):
                raise ValueError("Unsupported graphic line style")
            return base(
                el,
                "line",
                a,
                {"x": b["x"] - a["x"], "y": b["y"] - a["y"]},
                {"x": 0, "y": 0},
                style(el, dashed=el.get("LineType") == "Dashed", bold=el.get("LineType") == "Bold"),
            )
        if kind == "Rectangle":
            flags = set(el.get("RectangleType", "Plain").split())
            if flags - {"Plain", "RoundEdge", "Filled", "Dashed", "Bold"}:
                raise ValueError("Shadowed/shaded rectangle styles are not supported yet")
            return base(
                el,
                "rounded_rectangle" if "RoundEdge" in flags else "rectangle",
                origin,
                x,
                y,
                style(el, "Filled" in flags, "Dashed" in flags, "Bold" in flags),
            )
        if kind == "Bracket":
            label = el.get("BracketType", "RoundPair")
            names = {"SquarePair": "brackets", "RoundPair": "parentheses", "CurlyPair": "braces"}
            if label not in names:
                raise ValueError(
                    "Single CDXML brackets need explicit orientation; import as curves"
                )
            return base(el, names[label], origin, x, y, style(el))
        if kind == "Oval" and all(
            key in el.attrib for key in ("Center3D", "MajorAxisEnd3D", "MinorAxisEnd3D")
        ):
            center, major, minor = [
                point(el.attrib[key]) for key in ("Center3D", "MajorAxisEnd3D", "MinorAxisEnd3D")
            ]
            x = {k: 2 * (major[k] - center[k]) for k in ("x", "y")}
            y = {k: 2 * (minor[k] - center[k]) for k in ("x", "y")}
            origin = {k: center[k] - (x[k] + y[k]) / 2 for k in ("x", "y")}
            flags = set(el.get("OvalType", "Plain").split())
            if flags - {"Plain", "Circle", "Filled", "Dashed", "Bold"}:
                raise ValueError("Unsupported oval style")
            return base(
                el,
                "ellipse",
                origin,
                x,
                y,
                style(el, "Filled" in flags, "Dashed" in flags, "Bold" in flags),
            )
        raise ValueError("This CDXML graphic type is not supported yet; convert it to curves first")

    def group(el):
        children = list(el)
        if not children or any(child.tag != "curve" for child in children):
            raise ValueError("Only vector-curve groups are supported in this CDXML importer")
        parts = [curve(child) for child in children]
        if (
            len(parts) == 2
            and parts[0]["path"] == parts[1]["path"]
            and parts[0]["style"]["fill"] is not None
            and parts[1]["style"]["fill"] is None
        ):
            result = parts[1]
            result["style"]["fill"] = parts[0]["style"]["fill"]
        else:
            return None
        z = int(el.get("Z", "0"))
        result["layer"] = z - middle if z < middle else z - middle + 1
        return result

    result = []
    picture_budget = dict(bytes=0, pixels=0)

    def collect(container):
        for el in container:
            if el.tag in ("graphic", "curve", "group", "embeddedobject"):
                if el.get("SupersededBy") or el in objects:
                    continue
                if el.tag == "group":
                    value = (
                        group(el)
                        if list(el) and all(c.tag == "curve" and c not in objects for c in el)
                        else None
                    )
                    if value is None:
                        collect(el)
                        continue
                elif el.tag == "embeddedobject":
                    value = read_picture(el, scale, picture_budget, defer=local_pictures)
                    z = int(el.get("Z", "0"))
                    value["layer"] = z - middle if z < middle else z - middle + 1
                else:
                    value = {"curve": curve, "graphic": graphic}[el.tag](el)
                value["id"] = first_id + len(result)
                result.append(value)
                objects[el] = [value["id"]]
            elif el.tag == "fragment":
                # ChemDraw can move a curve into a molecular fragment when
                # saving. Retain its geometry without giving it chemical meaning.
                collect(el)

    for page in root.findall("page"):
        collect(page)
    return result


def write_graphics(
    page, graphics, paths, scale, position, color_id, next_id, parts=None, picture_exports=None
):
    """Emit editable Bézier curves; a two-curve group retains separate fill/stroke."""
    ordered = sorted(graphics, key=lambda g: g.get("layer", -1))
    middle = sum(g.get("layer", -1) < 0 for g in ordered) + 1
    objects = {}

    def curve_points(commands):
        groups, current, closed = [], [], False
        anchor = None
        for command in commands:
            kind, p = command["command"], command.get("points")
            if kind == "move":
                if current:
                    groups.append((current, closed))
                current, closed, anchor = [p, p, p], False, p
            elif kind in ("line", "cubic"):
                if not current:
                    raise ValueError("Graphic path does not start at an anchor")
                a, b, end = (anchor, p, p) if kind == "line" else p
                current[-1] = a
                current.extend([b, end, end])
                anchor = end
            elif kind == "close":
                closed = True
        if current:
            groups.append((current, closed))
        for points, closed in groups:
            if (
                closed
                and len(points) >= 9
                and all(
                    abs(a - b) < 1e-6
                    for a, b in zip(
                        (points[1]["x"], points[1]["y"]), (points[-2]["x"], points[-2]["y"])
                    )
                )
            ):
                points[0] = points[-3]
                del points[-3:]
        return groups

    for index, g in enumerate(ordered):
        z = index + 1 if g.get("layer", -1) < 0 else index + 2
        if g.get("kind") == "picture":
            prepared = None
            if picture_exports is not None:
                prepared = picture_exports.get(str(g["id"]))
                if not isinstance(prepared, str):
                    raise ValueError("Missing prepared picture export")
            objects[g["id"]] = write_picture(page, g, position, next_id, z, prepared)
            next_id += 1
            continue
        styled_parts = (parts or {}).get(str(g["id"]))
        if styled_parts is None:
            if isinstance(g.get("kind"), dict):
                raise ValueError(
                    "Scientific drawing export requires the editor's styled vector parts"
                )
            commands = (paths or {}).get(str(g["id"]))
            if commands is None:
                raise ValueError("CDXML graphic export requires the editor's vector geometry")
            styled_parts = [dict(commands=commands, style=g.get("style", {}))]
        parsed = []
        for part in styled_parts:
            paint = {
                "stroke": [0, 0, 0],
                "fill": None,
                "width_pt": 0.6,
                "pattern": "solid",
                **part["style"],
            }
            if paint["pattern"] == "dotted":
                raise ValueError(
                    "Dotted graphics are preserved in native/SVG/PDF/PNG; CDXML dotted strokes are not supported yet"
                )
            subpaths = curve_points(part["commands"])
            if not subpaths or any(len(points) < 6 for points, _ in subpaths):
                raise ValueError("A graphic path needs at least two anchors for CDXML export")
            if (
                paint["fill"] is not None
                and any(closed for _, closed in subpaths)
                and not all(closed for _, closed in subpaths)
            ):
                raise ValueError("Mixed open/closed filled paths are not supported by CDXML export")
            parsed.append((paint, subpaths))
        parent = page
        if len(parsed) > 1 or any(
            len(subpaths) > 1 or paint["fill"] is not None for paint, subpaths in parsed
        ):
            parent = ET.SubElement(page, "group", id=str(next_id), Z=str(z))
            next_id += 1

        def emit(target, points, closed, color, fill, width, dashed):
            nonlocal next_id
            flags = int(closed) | (2 if dashed else 0) | (128 if fill else 0)
            element = ET.SubElement(
                target,
                "curve",
                id=str(next_id),
                Z=str(z),
                CurvePoints=" ".join(position(p) for p in points),
                CurveType=str(flags),
                Closed="yes" if closed else "no",
                FillType="Solid" if fill else "None",
                LineType="Dashed" if dashed else "Solid",
                LineWidth=str(width),
                color=color_id(color),
            )
            next_id += 1
            objects[g["id"]] = parent if parent is not page else element

        for paint, subpaths in parsed:
            for points, closed in subpaths:
                target = parent
                if len(parsed) > 1 and paint["fill"] is not None:
                    target = ET.SubElement(parent, "group", id=str(next_id), Z=str(z))
                    next_id += 1
                if paint["fill"] is not None:
                    emit(target, points, closed, paint["fill"], True, 0, False)
                emit(
                    target,
                    points,
                    closed,
                    paint["stroke"],
                    False,
                    paint["width_pt"],
                    paint["pattern"] == "dashed",
                )
    return next_id, middle, objects
