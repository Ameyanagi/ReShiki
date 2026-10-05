"""Atom-label appearance and atom/bond-owned CDXML object tags.

CIP text is always recomputed from chemistry; imported text is never used as
stereochemical truth. Unsupported tags fail explicitly instead of detaching.
"""

import math
import xml.etree.ElementTree as ET


def inherited(element, parents):
    chain = []
    while element is not None:
        chain.append(element)
        element = parents.get(element)
    attrs = {}
    for element in reversed(chain):
        attrs.update(element.attrib)
    return attrs


def yes(attrs, name, default=False):
    value = attrs.get(name)
    if value is None:
        return default
    if value not in ("yes", "no"):
        raise ValueError("Invalid CDXML label setting: " + name)
    return value == "yes"


def carbons(attrs):
    terminal = yes(attrs, "ShowTerminalCarbonLabels")
    internal = yes(attrs, "ShowNonTerminalCarbonLabels")
    return (
        "all"
        if terminal and internal
        else "terminal"
        if terminal
        else "internal"
        if internal
        else "skeletal"
    )


def read_labels(root, mol, base, scale, read_text, object_map):
    parents = {child: parent for parent in root.iter() for child in parent}
    settings = dict(
        carbons=carbons(root.attrib),
        hydrogens=not yes(root.attrib, "HideImplicitHydrogens"),
        stereo=yes(root.attrib, "ShowAtomStereo"),
    )
    base["atom_labels"] = settings
    nodes = {}
    positions = {}
    for node in root.iter("n"):
        xy = [float(v) * scale for v in node.get("p", "").split()]
        if len(xy) != 2 or not all(math.isfinite(v) for v in xy):
            raise ValueError("Invalid atom label coordinates")
        matches = []
        for atom in mol.GetAtoms():
            p = mol.GetConformer().GetAtomPosition(atom.GetIdx())
            if (
                atom.GetAtomicNum() == int(node.get("Element", "6"))
                and abs(p.x * 28 - xy[0]) < 0.02
                and abs(-p.y * 28 - xy[1]) < 0.02
            ):
                matches.append(atom.GetIdx() + 1)
        if len(matches) != 1:
            raise ValueError("Could not safely associate atom labels with their atom")
        identifier = matches[0]
        nodes[node.get("id")] = identifier
        positions[identifier] = xy
        attrs = inherited(node, parents)
        display = dict(
            carbons=carbons(attrs),
            hydrogens=not yes(attrs, "HideImplicitHydrogens"),
            stereo=dict(show=yes(attrs, "ShowAtomStereo")),
        )
        # LabelAlignment describes the text anchor; only LabelDisplay is a
        # user-requested hydrogen placement override on the chemical node.
        position = attrs.get("LabelDisplay", "Auto")
        if position not in ("Auto", "Best", "Left", "Right", "Above", "Below"):
            raise ValueError("This atom-label alignment is not supported yet")
        display["hydrogen_position"] = {"Auto": "auto", "Best": "auto"}.get(
            position, position.lower()
        )
        if yes(attrs, "ShowAtomNumber") and node.get("AtomNumber"):
            display["number"] = dict(text=node.get("AtomNumber"))
        entry = next((a for a in base["atoms"] if a["id"] == identifier), None)
        if entry is None:
            entry = dict(id=identifier)
            base["atoms"].append(entry)
        entry["display"] = display
        object_map[node] = [identifier]
    for element in list(root.iter("n")) + list(root.iter("b")):
        attrs = inherited(element, parents)
        if element.tag == "n":
            identifier = nodes[element.get("id")]
            entry = next(a for a in base["atoms"] if a["id"] == identifier)
            target = entry["display"]
            anchor = positions[identifier]
            show_number = yes(attrs, "ShowAtomNumber")
        else:
            a, b = nodes.get(element.get("B")), nodes.get(element.get("E"))
            entry = next((bnd for bnd in base["bonds"] if {bnd["a"], bnd["b"]} == {a, b}), None)
            if entry is None:
                raise ValueError("A stereochemistry indicator refers to a missing bond")
            target = dict(stereo=dict(show=yes(attrs, "ShowBondStereo")))
            entry["indicator"] = target["stereo"]
            anchor = [(positions[a][i] + positions[b][i]) / 2 for i in (0, 1)]
            show_number = False
        seen = set()
        for tag in element.findall("objecttag"):
            name = tag.get("Name")
            if (
                name not in ("number", "stereo")
                or name in seen
                or (name == "number" and element.tag != "n")
            ):
                raise ValueError("Unsupported or duplicate atom/bond object tag")
            seen.add(name)
            texts = tag.findall("t")
            if len(texts) != 1:
                raise ValueError("An atom indicator must contain one text object")
            value, fmt = read_text(texts[0], attrs)
            if fmt["spans"] or fmt["style"]["script"] != "normal":
                raise ValueError("Mixed formatting in atom indicators is not supported yet")
            if not value or len(value) > 32 or any(ord(c) < 32 for c in value):
                raise ValueError("Invalid atom indicator text")
            if name == "number" and not show_number:
                continue
            if name == "stereo" and not target["stereo"]["show"]:
                continue
            item = dict(style=fmt["style"])
            box = [float(v) * scale for v in texts[0].get("BoundingBox", "").split()]
            if box:
                if len(box) != 4 or not all(math.isfinite(v) for v in box):
                    raise ValueError("Invalid atom indicator bounds")
                item["offset"] = dict(
                    x=(box[0] + box[2]) / 2 - anchor[0], y=(box[1] + box[3]) / 2 - anchor[1]
                )
            if name == "number":
                item["text"] = value
                target["number"] = item
            else:
                target["stereo"].update(item)
    if any(parents[tag].tag not in ("n", "b") for tag in root.iter("objecttag")):
        raise ValueError("Unattached object tags are not supported yet")


def carbon_attributes(value):
    return dict(
        ShowTerminalCarbonLabels="yes" if value in ("terminal", "all") else "no",
        ShowNonTerminalCarbonLabels="yes" if value in ("internal", "all") else "no",
    )


def visible(atom, doc):
    degree = sum(atom["id"] in (b["a"], b["b"]) for b in doc["bonds"])
    mode = atom.get("display", {}).get("carbons") or doc.get("atom_labels", {}).get(
        "carbons", "skeletal"
    )
    return (
        atom["element"] != "C"
        or atom.get("charge")
        or atom.get("isotope")
        or atom.get("radical_electrons")
        or degree == 0
        or mode == "all"
        or (mode == "terminal" and degree == 1)
        or (mode == "internal" and degree > 1)
    )


def write_labels(
    root, doc, object_map, bond_nodes, position, scale, write_text, defaults, indicators
):
    settings = doc.get("atom_labels", {})
    stereo = settings.get("stereo", False)
    root.attrib.update(carbon_attributes(settings.get("carbons", "skeletal")))
    root.set("HideImplicitHydrogens", "no" if settings.get("hydrogens", True) else "yes")
    root.set("ShowAtomStereo", "yes" if stereo else "no")
    root.set("ShowBondStereo", "yes" if stereo else "no")

    def indicator(node, owner, kind, text, data, anchor):
        # The desktop passes the same measured geometry used on canvas and in
        # vector/raster exports. Protocol clients may omit it; use a simple
        # offset then, retaining editable ownership and appearance.
        metrics = next(
            (i for i in indicators or [] if i["owner"] == owner and i["text"] == text), None
        )
        style = {**defaults, "size_pt": 7.5, "italic": kind == "stereo", **data.get("style", {})}
        width = len(text) * style["size_pt"] * 0.6 / scale
        height = style["size_pt"] / scale
        offset = data.get("offset") or dict(x=0, y=-height * 1.3)
        center = {k: anchor[k] + offset[k] for k in ("x", "y")}
        origin = dict(x=center["x"] - width / 2, y=center["y"] - height / 2)
        if metrics:
            width, height, origin, center = (
                metrics[k] for k in ("width", "height", "origin", "center")
            )
            style = metrics["style"]
        tag = ET.SubElement(node, "objecttag", Name=kind, TagType="Unknown")
        far = dict(x=origin["x"] + width, y=origin["y"] + height)
        baseline = dict(x=origin["x"], y=origin["y"] + height * 0.9)
        write_text(
            tag,
            text,
            dict(style=style),
            p=position(baseline),
            BoundingBox=position(origin) + " " + position(far),
            CaptionLineHeight="variable",
        )

    for atom in doc["atoms"]:
        node = object_map[atom["id"]]
        display = atom.get("display", {})
        if display.get("carbons") is not None:
            node.attrib.update(carbon_attributes(display["carbons"]))
        if display.get("hydrogens") is not None:
            node.set("HideImplicitHydrogens", "no" if display["hydrogens"] else "yes")
        hydrogen_position = display.get("hydrogen_position", "auto")
        node.set("LabelDisplay", hydrogen_position.capitalize())
        number = display.get("number")
        if number:
            node.set("AtomNumber", number["text"])
            node.set("ShowAtomNumber", "yes")
            indicator(
                node, {"Number": atom["id"]}, "number", number["text"], number, atom["position"]
            )
        data = display.get("stereo", {})
        show = stereo if data.get("show") is None else data["show"]
        node.set("ShowAtomStereo", "yes" if show else "no")
        if show and atom.get("cip_label"):
            indicator(
                node,
                {"AtomStereo": atom["id"]},
                "stereo",
                "(" + atom["cip_label"] + ")",
                data,
                atom["position"],
            )
    atoms = {a["id"]: a for a in doc["atoms"]}
    for bond, node in zip(doc["bonds"], bond_nodes):
        data = bond.get("indicator", {})
        show = stereo if data.get("show") is None else data["show"]
        node.set("ShowBondStereo", "yes" if show else "no")
        if show and bond.get("cip_label"):
            anchor = {
                k: (atoms[bond["a"]]["position"][k] + atoms[bond["b"]]["position"][k]) / 2
                for k in ("x", "y")
            }
            indicator(
                node,
                {"BondStereo": [bond["a"], bond["b"]]},
                "stereo",
                "(" + bond["cip_label"] + ")",
                data,
                anchor,
            )
