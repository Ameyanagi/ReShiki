"""Physical document styles shared by native drawings and editable exchange."""

import json
import math
from pathlib import Path

DEFAULT = json.loads(Path(__file__).with_name("drawing_style.json").read_text(encoding="utf-8"))
POINTS_PER_WORLD = 14.4 / 42


def checked(value=None):
    style = {**DEFAULT, **(value or {})}
    for name, minimum, maximum in [
        ("bond_length_pt", 5, 100),
        ("font_size_pt", 4, 144),
        ("line_width_pt", 0.1, 6),
        ("bold_width_pt", 0.1, 12),
        ("margin_width_pt", 0, 12),
        ("hash_spacing_pt", 0.3, 12),
        ("bond_spacing_ratio", 0.05, 0.4),
    ]:
        number = style[name]
        if (
            isinstance(number, bool)
            or not isinstance(number, (float, int))
            or not math.isfinite(number)
            or not minimum <= number <= maximum
        ):
            raise ValueError(f"Invalid drawing style {name}: expected {minimum}–{maximum}")
    for name, limit in [("name", 120), ("font_family", 256)]:
        if not isinstance(style[name], str) or not style[name].strip() or len(style[name]) > limit:
            raise ValueError(f"Invalid drawing style {name}")
    length = style["bond_length_world"]
    if not isinstance(length, (float, int)) or not math.isfinite(length):
        raise ValueError("Invalid drawing style coordinate units")
    if abs(length - style["bond_length_pt"] / POINTS_PER_WORLD) > 0.001:
        raise ValueError("Drawing style uses incompatible coordinate units")
    if style["bold_width_pt"] < style["line_width_pt"] or style["png_dpi"] != 1200:
        raise ValueError("Invalid drawing style bold width or image resolution")
    return style


def from_cdxml(root):
    fonts = {el.get("id"): el.get("name", "Arial") for el in root.findall("./fonttable/font")}
    style = dict(DEFAULT)
    for field, attribute in [
        ("bond_length_pt", "BondLength"),
        ("font_size_pt", "LabelSize"),
        ("line_width_pt", "LineWidth"),
        ("bold_width_pt", "BoldWidth"),
        ("margin_width_pt", "MarginWidth"),
        ("hash_spacing_pt", "HashSpacing"),
    ]:
        style[field] = float(
            root.get(attribute, "30" if attribute == "BondLength" else str(style[field]))
        )
    style["font_family"] = fonts.get(root.get("LabelFont"), "Arial")
    style["bond_spacing_ratio"] = float(root.get("BondSpacing", "18")) / 100
    style["bond_length_world"] = style["bond_length_pt"] / POINTS_PER_WORLD
    if style != DEFAULT:
        style["name"] = "Imported style"
    return checked(style)
