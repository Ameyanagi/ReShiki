"""Versioned JSON-lines chemistry service. No editor state or UI dependencies."""

import base64
import json
import math
import sys
import xml.etree.ElementTree as ET
from functools import partial
from pathlib import Path
from typing import TYPE_CHECKING, Any, TypedDict

from rdkit import Chem, rdBase
from rdkit.Chem import rdCIPLabeler, rdDepictor, rdMolDescriptors

if TYPE_CHECKING or __package__:
    from . import (
        abbreviations,
        abbreviations_exchange,
        aromatic,
        cleanup,
        drawing_styles,
        prepared,
        reactions,
    )
    from .arrows_exchange import read_arrow, write_arrow
    from .bonds_exchange import chemistry_xml, read_bonds, write_bond, write_crossings
    from .cdx_exchange import LIMIT as CDX_LIMIT
    from .cdx_exchange import from_cdx, to_cdx
    from .graphics_exchange import palette, read_graphics, write_graphics
    from .groups_exchange import read_groups, write_groups
    from .labels_exchange import read_labels, write_labels
    from .labels_exchange import visible as label_visible
    from .marks_exchange import read_marks, write_marks
else:
    import abbreviations
    import abbreviations_exchange
    import aromatic
    import cleanup
    import drawing_styles
    import prepared
    import reactions
    from arrows_exchange import read_arrow, write_arrow
    from bonds_exchange import chemistry_xml, read_bonds, write_bond, write_crossings
    from cdx_exchange import LIMIT as CDX_LIMIT
    from cdx_exchange import from_cdx, to_cdx
    from graphics_exchange import palette, read_graphics, write_graphics
    from groups_exchange import read_groups, write_groups
    from labels_exchange import read_labels, write_labels
    from labels_exchange import visible as label_visible
    from marks_exchange import read_marks, write_marks

SCALE = 28.0
DRAWING_STYLE = json.loads(
    Path(__file__).with_name("drawing_style.json").read_text(encoding="utf-8")
)
ORDERS = {
    1: Chem.BondType.SINGLE,
    2: Chem.BondType.DOUBLE,
    3: Chem.BondType.TRIPLE,
    4: Chem.BondType.AROMATIC,
    0: Chem.BondType.HYDROGEN,
    5: Chem.BondType.DATIVE,
    6: Chem.BondType.QUADRUPLE,
    7: Chem.BondType.ONEANDAHALF,
}
STEREO = {
    "cis": Chem.BondStereo.STEREOCIS,
    "trans": Chem.BondStereo.STEREOTRANS,
    "z": Chem.BondStereo.STEREOZ,
    "e": Chem.BondStereo.STEREOE,
}
DIRECTIONS = {
    "wedge": Chem.BondDir.BEGINWEDGE,
    "hash": Chem.BondDir.BEGINDASH,
    "wavy": Chem.BondDir.UNKNOWN,
    "plain": Chem.BondDir.NONE,
    "hollow_wedge": Chem.BondDir.BEGINWEDGE,
    "hashed": Chem.BondDir.BEGINDASH,
    "bold": Chem.BondDir.BEGINWEDGE,
    "dashed": Chem.BondDir.NONE,
    "dotted": Chem.BondDir.NONE,
}


def check_supported(mol):
    if mol.GetStereoGroups():
        raise ValueError("Enhanced stereo groups are not supported yet; import was cancelled.")
    for a in mol.GetAtoms():
        if a.HasQuery():
            raise ValueError("Query atoms are not supported yet; import was cancelled.")
        if a.GetNumRadicalElectrons() > 2:
            raise ValueError("More than two unpaired electrons on an atom are not supported yet")
        if a.GetChiralTag() not in (
            Chem.ChiralType.CHI_UNSPECIFIED,
            Chem.ChiralType.CHI_TETRAHEDRAL_CW,
            Chem.ChiralType.CHI_TETRAHEDRAL_CCW,
        ):
            raise ValueError("This stereochemistry class is not supported yet.")
    for b in mol.GetBonds():
        if b.HasQuery() or b.GetBondType() not in ORDERS.values():
            raise ValueError("This bond type is not supported yet.")
        if b.GetStereo() not in (
            *STEREO.values(),
            Chem.BondStereo.STEREONONE,
            Chem.BondStereo.STEREOANY,
        ):
            raise ValueError("This bond stereochemistry is not supported yet.")


def from_document(doc):
    reactions.validate(doc)
    if doc.get("version") not in range(1, 16):
        raise ValueError("Unsupported document version")
    drawing_styles.checked(doc.get("drawing_style"))
    abbreviations.validate(doc)
    rw = Chem.RWMol()
    ids = {}
    for item in doc["atoms"]:
        atom_id = item["id"]
        if atom_id in ids or not isinstance(atom_id, int) or atom_id < 1:
            raise ValueError("Atom IDs must be unique positive integers")
        a = Chem.Atom(item["element"])
        a.SetFormalCharge(item.get("charge", 0))
        a.SetNumRadicalElectrons(item.get("radical_electrons", 0))
        a.SetIsotope(item.get("isotope", 0))
        a.SetNumExplicitHs(item.get("explicit_h", 0))
        a.SetNoImplicit(item.get("no_implicit", False))
        a.SetIsAromatic(item.get("aromatic", False))
        a.SetAtomMapNum(item.get("map_num", 0))
        a.SetProp("reshiki_id", str(atom_id))
        ids[atom_id] = rw.AddAtom(a)
    for item in doc["bonds"]:
        if item["order"] == 0:
            atoms = {a["id"]: a for a in doc["atoms"]}
            h, acceptor = atoms[item["a"]], atoms[item["b"]]
            if (
                h["element"] != "H"
                or acceptor["element"] not in ("N", "O", "F", "S")
                or acceptor.get("charge", 0) > 0
                or not any(
                    b["order"] == 1
                    and (
                        (b["a"] == item["a"] and b["b"] != item["b"])
                        or (b["b"] == item["a"] and b["a"] != item["b"])
                    )
                    for b in doc["bonds"]
                )
            ):
                raise ValueError(
                    "A hydrogen bond must start at a covalently bound explicit H and end at an acceptor (N, O, F or S)"
                )
        rw.AddBond(ids[item["a"]], ids[item["b"]], ORDERS[item["order"]])
        b = rw.GetBondBetweenAtoms(ids[item["a"]], ids[item["b"]])
        display = item.get("display", "plain")
        if item["order"] == 1 or (item["order"] == 2 and display == "wavy"):
            b.SetBondDir(DIRECTIONS[display])
    mol = rw.GetMol()
    conf = Chem.Conformer(mol.GetNumAtoms())
    conf.Set3D(False)
    for item in doc["atoms"]:
        x, y = item["position"]["x"], item["position"]["y"]
        if not math.isfinite(x) or not math.isfinite(y):
            raise ValueError("Invalid atom coordinates")
        conf.SetAtomPosition(ids[item["id"]], (x / SCALE, -y / SCALE, 0.0))
        a = mol.GetAtomWithIdx(ids[item["id"]])
        stereo = item.get("stereo")
        if stereo:
            current = [int(n.GetProp("reshiki_id")) for n in a.GetNeighbors()]
            given = stereo["neighbors"]
            if set(current) != set(given) or len(current) != len(given):
                raise ValueError("Stereocenter neighbor mapping changed")
            permutation = [given.index(n) for n in current]
            odd = (
                sum(
                    permutation[i] > permutation[j]
                    for i in range(len(permutation))
                    for j in range(i + 1, len(permutation))
                )
                % 2
            )
            clockwise = stereo["winding"] == "cw"
            if odd:
                clockwise = not clockwise
            a.SetChiralTag(
                Chem.ChiralType.CHI_TETRAHEDRAL_CW
                if clockwise
                else Chem.ChiralType.CHI_TETRAHEDRAL_CCW
            )
    mol.AddConformer(conf)
    Chem.SanitizeMol(mol)
    for item in doc["atoms"]:
        requested = item.get("radical_electrons", 0)
        if requested and mol.GetAtomWithIdx(ids[item["id"]]).GetNumRadicalElectrons() != requested:
            raise ValueError("Radical count conflicts with the atom valence or explicit hydrogens")
    Chem.AssignChiralTypesFromBondDirs(mol, replaceExistingTags=False)
    for item in doc["bonds"]:
        b = mol.GetBondBetweenAtoms(ids[item["a"]], ids[item["b"]])
        if item.get("stereo") in STEREO:
            neighbors = item["stereo_atoms"]
            b.SetStereoAtoms(ids[neighbors[0]], ids[neighbors[1]])
            b.SetStereo(STEREO[item["stereo"]])
        elif item.get("display") == "wavy" and item["order"] == 2:
            b.SetStereo(Chem.BondStereo.STEREOANY)
    # Infer double-bond stereo from drawn geometry where it is unspecified.
    Chem.DetectBondStereochemistry(mol, confId=0)
    Chem.AssignStereochemistry(mol, cleanIt=False, force=True)
    check_supported(mol)
    return mol


def to_document(mol, base=None, rewedge=False):
    check_supported(mol)
    if not mol.GetNumConformers():
        rdDepictor.Compute2DCoords(mol)
    work = Chem.Mol(mol)
    Chem.Kekulize(work, clearAromaticFlags=True)
    Chem.WedgeMolBonds(work, work.GetConformer())
    for item in list(work.GetAtoms()) + list(work.GetBonds()):
        if item.HasProp("_CIPCode"):
            item.ClearProp("_CIPCode")
    rdCIPLabeler.AssignCIPLabels(work, maxRecursiveIterations=1_250_000)
    ids = {
        a.GetIdx(): int(a.GetProp("reshiki_id")) if a.HasProp("reshiki_id") else a.GetIdx() + 1
        for a in work.GetAtoms()
    }
    atoms: list[dict[str, Any]] = []
    for a in work.GetAtoms():
        p = work.GetConformer().GetAtomPosition(a.GetIdx())
        stereo = None
        if a.GetChiralTag() != Chem.ChiralType.CHI_UNSPECIFIED:
            stereo = {
                "winding": "cw"
                if a.GetChiralTag() == Chem.ChiralType.CHI_TETRAHEDRAL_CW
                else "ccw",
                "neighbors": [ids[n.GetIdx()] for n in a.GetNeighbors()],
            }
        atoms.append(
            {
                "id": ids[a.GetIdx()],
                "element": a.GetSymbol(),
                "position": {"x": p.x * SCALE, "y": -p.y * SCALE},
                "charge": a.GetFormalCharge(),
                "isotope": a.GetIsotope(),
                "radical_electrons": a.GetNumRadicalElectrons(),
                "explicit_h": a.GetNumExplicitHs(),
                "no_implicit": a.GetNoImplicit(),
                "aromatic": a.GetIsAromatic(),
                "map_num": a.GetAtomMapNum(),
                "stereo": stereo,
                "label_h": a.GetTotalNumHs(),
                "cip_label": a.GetProp("_CIPCode") if a.HasProp("_CIPCode") else None,
            }
        )
    previous = {a["id"]: a for a in (base or {}).get("atoms", [])}
    circular_atoms = {
        id for b in (base or {}).get("bonds", []) if b["order"] == 4 for id in (b["a"], b["b"])
    }
    index_by_id = {id: i for i, id in ids.items()}
    for atom in atoms:
        if atom["id"] in circular_atoms:
            atom["explicit_h"] = mol.GetAtomWithIdx(index_by_id[atom["id"]]).GetNumExplicitHs()
        if previous.get(atom["id"], {}).get("display"):
            atom["display"] = previous[atom["id"]]["display"]
        if previous.get(atom["id"], {}).get("marks"):
            atom["marks"] = previous[atom["id"]]["marks"]
        if previous.get(atom["id"], {}).get("text_style"):
            atom["text_style"] = previous[atom["id"]]["text_style"]
    bonds = []
    previous_bonds = {frozenset((b["a"], b["b"])): b for b in (base or {}).get("bonds", [])}
    reverse_stereo = {value: key for key, value in STEREO.items()}
    for b in work.GetBonds():
        display = {
            Chem.BondDir.BEGINWEDGE: "wedge",
            Chem.BondDir.BEGINDASH: "hash",
            Chem.BondDir.UNKNOWN: "wavy",
        }.get(b.GetBondDir(), "plain")
        if b.GetStereo() == Chem.BondStereo.STEREOANY:
            display = "wavy"
        a, z = ids[b.GetBeginAtomIdx()], ids[b.GetEndAtomIdx()]
        old = previous_bonds.get(frozenset((a, z)))
        stereo_atoms = [ids[i] for i in b.GetStereoAtoms()]
        appearance = {}
        if old:
            appearance = {
                k: old[k]
                for k in ("secondary_display", "double_position", "color", "indicator", "z_order")
                if k in old
            }
            display = old.get("display", "plain")
            if (
                rewedge
                and old["order"] == 1
                and display in ("wedge", "hash", "hollow_wedge", "hashed", "bold")
            ):
                start = next(i for i, id in ids.items() if id == old["a"])
                if work.GetAtomWithIdx(start).GetChiralTag() != Chem.ChiralType.CHI_UNSPECIFIED:
                    Chem.WedgeBond(b, start, work.GetConformer())
                    up = b.GetBondDir() == Chem.BondDir.BEGINWEDGE
                    if display == "bold":
                        display = "bold" if up else "hashed"
                    elif display in ("hollow_wedge", "hashed"):
                        display = "hollow_wedge" if up else "hashed"
                    else:
                        display = "wedge" if up else "hash"
            if old["a"] != a:
                stereo_atoms.reverse()
            a, z = old["a"], old["b"]
        bonds.append(
            {
                "a": a,
                "b": z,
                "order": 4
                if old and old["order"] == 4 and mol.GetBondWithIdx(b.GetIdx()).GetIsAromatic()
                else {v: k for k, v in ORDERS.items()}[b.GetBondType()],
                "display": display,
                "stereo": reverse_stereo.get(b.GetStereo()),
                "stereo_atoms": stereo_atoms,
                "cip_label": b.GetProp("_CIPCode") if b.HasProp("_CIPCode") else None,
                **appearance,
            }
        )
    old_order = {
        frozenset((b["a"], b["b"])): i for i, b in enumerate((base or {}).get("bonds", []))
    }
    bonds.sort(key=lambda b: old_order.get(frozenset((b["a"], b["b"])), len(old_order)))
    return {
        "version": 15,
        "drawing_style": drawing_styles.checked((base or {}).get("drawing_style")),
        "atoms": atoms,
        "bonds": bonds,
        **(
            {"page_layout": base["page_layout"]}
            if base and base.get("page_layout") is not None
            else {}
        ),
        "abbreviations": (base or {}).get("abbreviations", []),
        "atom_labels": (base or {}).get("atom_labels", {}),
        "annotations": (base or {}).get("annotations", []),
        "arrows": (base or {}).get("arrows", []),
        "graphics": (base or {}).get("graphics", []),
        "groups": (base or {}).get("groups", []),
        "reactions": (base or {}).get("reactions", []),
    }


def analyze(mol, *, local_properties=False):
    identifiers = not any(
        b.GetBondType() in (Chem.BondType.HYDROGEN, Chem.BondType.ONEANDAHALF)
        for b in mol.GetBonds()
    )
    inchi_ok = identifiers and not any(
        b.GetBondType() in (Chem.BondType.DATIVE, Chem.BondType.QUADRUPLE) for b in mol.GetBonds()
    )
    result = {
        "smiles": Chem.MolToSmiles(mol) if identifiers else "",
        "inchi": Chem.MolToInchi(mol) if inchi_ok else "",
        "inchikey": Chem.MolToInchiKey(mol) if inchi_ok else "",
    }
    if local_properties:
        # Supply the sanitized graph, not toolkit-computed H counts. Rust owns
        # the final valence/H calculation. Keep the original path as the oracle.
        result["property_input"] = {
            "rdkit_version": rdBase.rdkitVersion,
            # Retain only as a fallback while Rust certifies equal-sized ring
            # pruning. Some dense graphs depend on the C++ sort implementation.
            "reference_rings": list(mol.GetRingInfo().AtomRings()),
            "graph": {
                "atoms": [
                    {
                        "atomic_number": a.GetAtomicNum(),
                        "isotope": a.GetIsotope(),
                        "charge": a.GetFormalCharge(),
                        "explicit_hydrogens": a.GetNumExplicitHs(),
                        "no_implicit": a.GetNoImplicit(),
                        "aromatic": a.GetIsAromatic(),
                        "radical_electrons": a.GetNumRadicalElectrons(),
                    }
                    for a in mol.GetAtoms()
                ],
                "bonds": [
                    {
                        "a": b.GetBeginAtomIdx(),
                        "b": b.GetEndAtomIdx(),
                        "order": {v: k for k, v in ORDERS.items()}[b.GetBondType()],
                        "aromatic": b.GetIsAromatic(),
                    }
                    for b in mol.GetBonds()
                ],
            },
        }
    else:
        result.update(
            logp=rdMolDescriptors.CalcCrippenDescriptors(mol)[0],
            tpsa=rdMolDescriptors.CalcTPSA(mol),
            donors=rdMolDescriptors.CalcNumHBD(mol),
            acceptors=rdMolDescriptors.CalcNumHBA(mol),
            rings=rdMolDescriptors.CalcNumRings(mol),
            formula=rdMolDescriptors.CalcMolFormula(mol),
            mass=rdMolDescriptors._CalcMolWt(mol),
            exact_mass=rdMolDescriptors.CalcExactMolWt(mol),
            unpaired_electrons=sum(a.GetNumRadicalElectrons() for a in mol.GetAtoms()),
        )
    return result


class TextStyle(TypedDict):
    family: str
    size_pt: float
    bold: bool
    italic: bool
    underline: bool
    color: list[int]
    script: str
    formula: bool


TEXT_DEFAULTS: TextStyle = {
    "family": "Arial",
    "size_pt": 10.0,
    "bold": False,
    "italic": False,
    "underline": False,
    "color": [0, 0, 0],
    "script": "normal",
    "formula": False,
}


def text_runs(text, format):
    """Native style offsets are UTF-8 bytes, not Python character indices."""
    raw = text.encode("utf-8")
    base = {**TEXT_DEFAULTS, **format.get("style", {})}
    end = 0
    for span in format.get("spans", []):
        if not end <= span["start"] < span["end"] <= len(raw):
            raise ValueError("Invalid text style range")
        if span["start"] > end:
            yield raw[end : span["start"]].decode("utf-8"), base
        yield raw[span["start"] : span["end"]].decode("utf-8"), {**base, **span["style"]}
        end = span["end"]
    if end < len(raw) or not raw:
        yield raw[end:].decode("utf-8"), base


def cdxml_text_reader(root):
    fonts = {el.get("id"): el.get("name", "Arial") for el in root.findall("./fonttable/font")}
    colors = [[0, 0, 0], [255, 255, 255]] + (
        [
            [round(float(el.get(axis, "0")) * 255) for axis in ("r", "g", "b")]
            for el in root.findall("./colortable/color")
        ]
        or [[255, 255, 255], [0, 0, 0]]
    )

    def read(el, defaults=None, atom=False):
        inherited = {**root.attrib, **(defaults or {}), **el.attrib}
        prefix = "Label" if atom else "Caption"

        def style(run):
            face = int(run.get("face", inherited.get(prefix + "Face", "0")))
            if face & ~(1 | 2 | 4 | 32 | 64):
                raise ValueError("Outlined/shadowed CDXML text is not supported yet.")
            script = face & 96
            color = int(run.get("color", inherited.get(prefix + "Color", "3")))
            size = float(run.get("size", inherited.get(prefix + "Size", "10")))
            if not 0 <= color < len(colors) or not 4 <= size <= 144:
                raise ValueError("Unsupported CDXML text color or font size")
            return {
                "family": fonts.get(run.get("font", inherited.get(prefix + "Font")), "Arial"),
                "size_pt": size,
                "bold": bool(face & 1),
                "italic": bool(face & 2),
                "underline": bool(face & 4),
                "color": colors[color],
                "script": {32: "subscript", 64: "superscript"}.get(script, "normal"),
                "formula": script == 96,
            }

        runs = [(run.text or "", style(run)) for run in el.findall("s")]
        if not runs:
            raise ValueError("CDXML text has no supported style runs")
        text = "".join(value for value, _ in runs).replace("\r\n", "\n").replace("\r", "\n")
        base = runs[0][1]
        spans, offset = [], 0
        for value, run_style in runs:
            value = value.replace("\r\n", "\n").replace("\r", "\n")
            end = offset + len(value.encode("utf-8"))
            if run_style != base and end > offset:
                spans.append({"start": offset, "end": end, "style": run_style})
            offset = end
        alignment = inherited.get(prefix + "Justification", inherited.get("Justification", "Left"))
        if not atom and alignment not in ("Left", "Center", "Right", "Full"):
            raise ValueError("This CDXML text alignment is not supported yet")
        height = inherited.get(prefix + "LineHeight", inherited.get("LineHeight", "auto"))
        spacing = (
            1.2 if height in ("auto", "variable", "0", "1") else float(height) / base["size_pt"]
        )
        width = float(inherited.get("WordWrapWidth", "0"))
        if not 0.8 <= spacing <= 3.0 or (width and not 10 <= width <= 2000):
            raise ValueError("This CDXML paragraph spacing or width is not supported yet")
        if float(inherited.get("RotationAngle", "0")) != 0:
            raise ValueError("Rotated CDXML text is not supported yet")
        return text, {
            "style": base,
            "spans": spans,
            "alignment": {"Center": "center", "Right": "right", "Full": "justified"}.get(
                alignment, "left"
            ),
            "line_spacing": spacing,
            "width_pt": width or None,
        }

    return read


def import_cdxml(text, *, local_pictures=False):
    root = ET.fromstring(text)
    # These predicates and reaction changes have no equivalent in the drawing
    # model yet. A topology parser may ignore them, so reject them before it runs.
    predicates = {
        "RingBondCount": {"Unspecified", "-1"},
        "UnsaturatedBonds": {"Unspecified", "0"},
        "SubstituentsUpTo": set(),
        "SubstituentsExactly": set(),
        "FreeSites": {"0"},
        "LinkCountLow": set(),
        "LinkCountHigh": set(),
        "IsotopicAbundance": {"Unspecified", "0"},
        "Topology": {"Unspecified", "0"},
        "RxnChange": {"no", "0"},
        "RxnStereo": {"Unspecified", "0"},
        "RxnParticipation": {"Unspecified", "0"},
    }
    for el in root.iter():
        if el.tag in ("n", "b"):
            for name, defaults in predicates.items():
                if name in el.attrib and el.get(name) not in defaults:
                    raise ValueError("Unsupported query or reaction predicate: " + name)
    allowed = {
        "CDXML",
        "page",
        "fragment",
        "n",
        "b",
        "t",
        "s",
        "fonttable",
        "font",
        "colortable",
        "color",
        "arrow",
        "graphic",
        "curve",
        "group",
        "represent",
        "objecttag",
        "embeddedobject",
    }
    if {el.tag for el in root.iter()} - allowed or len(list(root.iter("page"))) != 1:
        raise ValueError("CDXML contains unsupported drawing objects or multiple pages.")
    abbreviated = abbreviations_exchange.flatten(root)
    parts = Chem.MolsFromCDXML(
        ET.tostring(chemistry_xml(root), encoding="unicode"), sanitize=False, removeHs=False
    )
    object_map = {}
    fragments = {el.get("id"): el for el in root.iter("fragment")}
    offset = 0
    for part in parts:
        if part.HasProp("CDX_FRAG_ID"):
            fragment = fragments.get(part.GetProp("CDX_FRAG_ID"))
            if fragment is not None:
                object_map[fragment] = list(range(offset + 1, offset + part.GetNumAtoms() + 1))
        offset += part.GetNumAtoms()
    mol = parts[0] if parts else Chem.Mol()
    for part in parts[1:]:
        mol = Chem.CombineMols(mol, part)
    document_style = drawing_styles.from_cdxml(root)
    scale = 1 / drawing_styles.POINTS_PER_WORLD
    # RDKit normalizes imported bonds to 1.5; restore the original physical size.
    factor = document_style["bond_length_pt"] / DRAWING_STYLE["bond_length_pt"]
    for conf in mol.GetConformers():
        for index in range(mol.GetNumAtoms()):
            p = conf.GetAtomPosition(index)
            conf.SetAtomPosition(index, (p.x * factor, p.y * factor, p.z * factor))
    base = {
        "drawing_style": document_style,
        "atoms": [],
        "annotations": [],
        "arrows": [],
        "bonds": read_bonds(root, parts, scale, palette(root)),
    }
    for bond in base["bonds"]:
        chemical = mol.GetBondBetweenAtoms(bond["a"] - 1, bond["b"] - 1)
        chemical.SetBondType(ORDERS[bond["order"]])
        if bond["order"] == 4:
            chemical.SetIsAromatic(True)
            chemical.GetBeginAtom().SetIsAromatic(True)
            chemical.GetEndAtom().SetIsAromatic(True)
    Chem.SanitizeMol(mol)
    Chem.AssignChiralTypesFromBondDirs(mol, replaceExistingTags=False)
    Chem.DetectBondStereochemistry(mol, confId=0)
    Chem.AssignStereochemistry(mol, cleanIt=False, force=True)
    read_text = cdxml_text_reader(root)
    next_id = mol.GetNumAtoms() + 1

    def point(value):
        values = [float(v) for v in value.split()]
        if len(values) < 2 or not all(math.isfinite(v) for v in values):
            raise ValueError("Invalid CDXML coordinates")
        return {"x": values[0] * scale, "y": values[1] * scale}

    parents = {child: parent for parent in root.iter() for child in parent}
    for page in root.iter("page"):
        for el in page.iter():
            if el.tag == "t" and parents[el].tag not in ("n", "objecttag"):
                value, format = read_text(el, page.attrib)
                # Caption p is a baseline anchor; a bounding box supplies the
                # top-left origin used by the editor, including centered text.
                origin = el.get("BoundingBox", el.attrib.get("p", "0 0"))
                base["annotations"].append(
                    {"id": next_id, "position": point(origin), "text": value, "format": format}
                )
                object_map[el] = [next_id]
                next_id += 1
            elif el.tag == "arrow" or (
                el.tag == "curve"
                and any(
                    el.get(k, "None") not in ("None", "Unspecified")
                    for k in ("ArrowheadHead", "ArrowheadTail")
                )
            ):
                base["arrows"].append(read_arrow(el, root, point, palette(root), next_id))
                object_map[el] = [next_id]
                next_id += 1
    # Match styled atomic labels by element and their original CDXML coordinates.
    # RDKit does not retain CDXML IDs. Refuse ambiguous matches instead of styling
    # another atom, for example when two atoms have identical coordinates.
    for node in root.iter("n"):
        label = node.find("t")
        if label is None:
            if not any(
                key in node.attrib for key in ("LabelFont", "LabelSize", "LabelFace", "LabelColor")
            ):
                continue
            label = ET.Element("t")
            ET.SubElement(label, "s")
        _, format = read_text(label, node.attrib, atom=True)
        s = {**format["style"], "script": "normal", "formula": False}
        for span in format["spans"]:
            if {**span["style"], "script": "normal", "formula": False} != s:
                raise ValueError("Mixed fonts within a CDXML atom label are not supported yet")
        if s == TEXT_DEFAULTS:
            continue
        p = point(node.attrib["p"])
        matches = []
        for a in mol.GetAtoms():
            pos = mol.GetConformer().GetAtomPosition(a.GetIdx())
            if (
                a.GetAtomicNum() == int(node.get("Element", "6"))
                and abs(pos.x * SCALE - p["x"]) < 0.01
                and abs(-pos.y * SCALE - p["y"]) < 0.01
            ):
                matches.append(a.GetIdx() + 1)
        if len(matches) != 1:
            raise ValueError("Could not safely associate CDXML text style with its atom")
        base["atoms"].append({"id": matches[0], "text_style": s})
    read_marks(root, mol, base, scale, object_map)
    read_labels(root, mol, base, scale, read_text, object_map)
    aromatic.remove_owned_circles(root, mol, base, scale)
    base["graphics"] = read_graphics(
        root, scale, next_id, object_map, local_pictures=local_pictures
    )
    # A chemical fragment may also contain nonchemical curves.
    for fragment, atoms in list(object_map.items()):
        if fragment.tag == "fragment":
            object_map[fragment] = atoms + [
                id for child in fragment if child in object_map for id in object_map[child]
            ]
    base["groups"] = read_groups(root, object_map, next_id + len(base["graphics"]))
    base["abbreviations"] = abbreviations_exchange.read(abbreviated, root, mol, scale)
    if (
        not mol.GetNumAtoms()
        and not base["annotations"]
        and not base["arrows"]
        and not base["graphics"]
    ):
        raise ValueError("No supported drawing objects found")
    return mol, base


def export_cdxml(
    doc,
    text_layout=None,
    graphic_paths=None,
    graphic_parts=None,
    atom_indicators=None,
    picture_exports=None,
):
    # Coordinates and styles belong to the editor. Chemistry is checked first.
    mol = from_document(doc)
    style = drawing_styles.checked(doc.get("drawing_style"))
    scale = style["bond_length_pt"] / style["bond_length_world"]
    root = ET.Element(
        "CDXML",
        BondLength=str(style["bond_length_pt"]),
        LabelSize=str(style["font_size_pt"]),
        CaptionSize=str(style["font_size_pt"]),
        LabelFont="3",
        CaptionFont="3",
        LabelFace="0",
        CaptionFace="0",
        LineWidth=str(style["line_width_pt"]),
        BoldWidth=str(style["bold_width_pt"]),
        MarginWidth=str(style["margin_width_pt"]),
        HashSpacing=str(style["hash_spacing_pt"]),
        BondSpacing=str(style["bond_spacing_ratio"] * 100),
        ChainAngle="120",
    )
    fonts = ET.SubElement(root, "fonttable")
    colors = ET.SubElement(root, "colortable")
    font_ids, color_ids = {}, {}

    def font_id(name):
        if name not in font_ids:
            font_ids[name] = str(len(font_ids) + 3)
            ET.SubElement(fonts, "font", id=font_ids[name], charset="utf-8", name=name)
        return font_ids[name]

    def color_id(rgb):
        key = tuple(rgb)
        if key not in color_ids:
            color_ids[key] = str(len(color_ids) + 2)
            ET.SubElement(
                colors, "color", {axis: f"{c / 255:.8f}" for axis, c in zip(("r", "g", "b"), rgb)}
            )
        return color_ids[key]

    font_id(style["font_family"])
    color_id([255, 255, 255])
    color_id([0, 0, 0])

    def write_text(parent, value, format, **attrs):
        t = ET.SubElement(parent, "t", **attrs)
        for value, s in text_runs(value, format):
            face = int(s["bold"]) + 2 * int(s["italic"]) + 4 * int(s["underline"])
            face |= {"subscript": 32, "superscript": 64}.get(s["script"], 96 if s["formula"] else 0)
            ET.SubElement(
                t,
                "s",
                font=font_id(s["family"]),
                size=str(s["size_pt"]),
                face=str(face),
                color=color_id(s["color"]),
            ).text = value
        return t

    page = ET.SubElement(root, "page", id="1", BoundingBox="0 0 612 792")
    fragment = ET.SubElement(page, "fragment", id="2")
    object_map = {}
    points = [a["position"] for a in doc["atoms"]]
    # Fit all drawing objects into the page. In particular, a rotated picture
    # can extend above/left of its molecule; using atoms alone clips that image.
    points.extend(a["position"] for a in doc.get("annotations", []))
    for arrow in doc.get("arrows", []):
        points.extend(arrow[k] for k in ("start", "end", "control") if arrow.get(k))
    for graphic in doc.get("graphics", []):
        if graphic.get("kind") == "picture":
            o, x, y = [graphic[k] for k in ("origin", "axis_x", "axis_y")]
            points.extend(
                {k: o[k] + u * x[k] + v * y[k] for k in ("x", "y")}
                for u, v in ((0, 0), (1, 0), (0, 1), (1, 1))
            )
        else:
            for command in (graphic_paths or {}).get(str(graphic["id"]), []):
                value = command.get("points")
                if isinstance(value, dict):
                    points.append(value)
                elif isinstance(value, list):
                    points.extend(value)
    dx = 30 - min((p["x"] * scale for p in points), default=0)
    dy = 30 - min((p["y"] * scale for p in points), default=0)

    def position(p):
        return f"{p['x'] * scale + dx:.6f} {p['y'] * scale + dy:.6f}"

    ids = {a["id"]: i + 3 for i, a in enumerate(doc["atoms"])}
    for a in doc["atoms"]:
        attrs = {
            "id": str(ids[a["id"]]),
            "p": position(a["position"]),
            "Element": str(Chem.GetPeriodicTable().GetAtomicNumber(a["element"])),
        }
        if a.get("charge"):
            attrs["Charge"] = str(a["charge"])
        if a.get("isotope"):
            attrs["Isotope"] = str(a["isotope"])
        if a.get("radical_electrons"):
            attrs["Radical"] = {1: "Doublet", 2: "Triplet"}[a["radical_electrons"]]
        if a.get("explicit_h"):
            attrs["NumHydrogens"] = str(a["explicit_h"])
        if a.get("marks"):
            attrs["NumHydrogens"] = str(a.get("label_h", a.get("explicit_h", 0)))
        n = ET.SubElement(fragment, "n", attrs)
        object_map[a["id"]] = n
        if a.get("text_style") or a.get("marks") or a.get("display") or doc.get("atom_labels"):
            # ChemDraw requires a concrete label to resolve represent links.
            # Atomic identity stays in Element/Charge/Isotope; the text is a view.
            s = {
                **TEXT_DEFAULTS,
                "family": style["font_family"],
                "size_pt": style["font_size_pt"],
                **(a.get("text_style") or {}),
                "script": "normal",
                "formula": False,
            }
            isotope = str(a["isotope"]) if a.get("isotope") else ""
            label = isotope + a["element"]
            spans = []
            if isotope:
                spans.append(
                    {"start": 0, "end": len(isotope), "style": {**s, "script": "superscript"}}
                )
            hydrogens = a.get("label_h", 0)
            show_h = a.get("display", {}).get("hydrogens")
            if show_h is None:
                show_h = doc.get("atom_labels", {}).get("hydrogens", True)
            if hydrogens and show_h and a["element"] != "H":
                label += "H"
                if hydrogens > 1:
                    start = len(label)
                    label += str(hydrogens)
                    spans.append(
                        {"start": start, "end": len(label), "style": {**s, "script": "subscript"}}
                    )
            if a.get("charge") and not any(
                m["kind"] in ("charge", "circled_charge", "radical_ion") for m in a.get("marks", [])
            ):
                start = len(label)
                charge = a["charge"]
                label += (str(abs(charge)) if abs(charge) > 1 else "") + (
                    "+" if charge > 0 else "−"
                )
                spans.append(
                    {
                        "start": start,
                        "end": len(label.encode("utf-8")),
                        "style": {**s, "script": "superscript"},
                    }
                )
            # Invisible skeletal carbons need no text object. Keep their dormant
            # style on the node for ReShiki's editable CDXML round trip.
            hidden = not label_visible(a, doc)
            if hidden:
                face = int(s["bold"]) + 2 * int(s["italic"]) + 4 * int(s["underline"])
                n.attrib.update(
                    LabelFont=font_id(s["family"]),
                    LabelSize=str(s["size_pt"]),
                    LabelFace=str(face),
                    LabelColor=color_id(s["color"]),
                )
                continue
            write_text(
                n,
                label,
                {"style": s, "spans": spans},
                p=position(a["position"]),
                LabelAlignment="Auto",
            )
    next_id = len(ids) + 3
    bond_nodes = []
    for b in doc["bonds"]:
        attrs = {
            "id": str(next_id),
            "B": str(ids[b["a"]]),
            "E": str(ids[b["b"]]),
            "Order": {0: "hydrogen", 4: "1.5", 5: "dative", 6: "4", 7: "1.5"}.get(
                b["order"], str(b["order"])
            ),
        }
        attrs.update(write_bond(b, color_id))
        bond_nodes.append(ET.SubElement(fragment, "b", attrs))
        next_id += 1
    next_id = aromatic.write_circles(fragment, doc, mol, position, color_id, next_id)
    write_labels(
        root,
        doc,
        object_map,
        bond_nodes,
        position,
        scale,
        write_text,
        TEXT_DEFAULTS,
        atom_indicators,
    )
    for a in doc.get("annotations", []):
        format = a.get("format", {})
        s = {**TEXT_DEFAULTS, **format.get("style", {})}
        metrics = (text_layout or {}).get(str(a["id"]))
        if metrics is None:
            # Non-GUI protocol clients can supply measured metrics. This fallback
            # still preserves the origin; the desktop always supplies real ones.
            metrics = {
                "width": format.get("width_pt")
                or max(map(len, a["text"].split("\n"))) * s["size_pt"] * 0.6,
                "height": len(a["text"].split("\n"))
                * s["size_pt"]
                * format.get("line_spacing", 1.2),
                "baseline": s["size_pt"] * 0.9,
            }
        anchor = {"x": a["position"]["x"], "y": a["position"]["y"] + metrics["baseline"] / scale}
        anchor["x"] += (
            metrics["width"]
            / scale
            * {"center": 0.5, "right": 1.0}.get(format.get("alignment"), 0.0)
        )
        far = {
            "x": a["position"]["x"] + metrics["width"] / scale,
            "y": a["position"]["y"] + metrics["height"] / scale,
        }
        attrs = {
            "id": str(next_id),
            "p": position(anchor),
            "BoundingBox": position(a["position"]) + " " + position(far),
            "CaptionJustification": {"center": "Center", "right": "Right", "justified": "Full"}.get(
                format.get("alignment"), "Left"
            ),
            "CaptionLineHeight": str(round(s["size_pt"] * format.get("line_spacing", 1.2), 3)),
        }
        if format.get("width_pt"):
            attrs["WordWrapWidth"] = str(round(format["width_pt"]))
        object_map[a["id"]] = write_text(page, a["text"], format, **attrs)
        next_id += 1
    next_id = write_marks(
        fragment, doc["atoms"], ids, position, scale, next_id, style["font_size_pt"]
    )
    for a in doc.get("arrows", []):
        object_map[a["id"]] = write_arrow(page, a, next_id, position, color_id)
        next_id += 1
    next_id, middle, graphic_objects = write_graphics(
        page,
        doc.get("graphics", []),
        graphic_paths,
        scale,
        position,
        color_id,
        next_id,
        graphic_parts,
        picture_exports,
    )
    # Leave one common layer for the molecule, arrows and text. The graphic
    # layers above and below it retain the editor's front/back order.
    for el in page.iter():
        if el.tag in ("fragment", "n", "b", "t", "arrow") or el in [
            object_map[a["id"]] for a in doc.get("arrows", [])
        ]:
            el.set("Z", str(middle))
    write_crossings(doc, bond_nodes, page, middle)
    object_map.update(graphic_objects)
    write_groups(page, doc, fragment, object_map, ids, next_id)
    abbreviations_exchange.write(root, doc, ids, position, write_text, TEXT_DEFAULTS)
    return '<?xml version="1.0" encoding="UTF-8"?>\n' + ET.tostring(root, encoding="unicode")


def handle(request):
    if request.get("protocol") != 1:
        raise ValueError("Unsupported protocol version")
    prepared_reaction = request.get("prepared_reaction", False)
    if not isinstance(prepared_reaction, bool):
        raise ValueError("prepared_reaction must be a boolean")
    if prepared_reaction and (
        request.get("operation") != "import"
        or request.get("format") != "rxn"
        or not isinstance(request.get("document"), dict)
        or not isinstance(request.get("prepared_molecule"), dict)
        or any(
            request.get(key) is not None
            for key in ("prepared_parts", "prepared_import", "prepared_drawing")
        )
    ):
        raise ValueError("Prepared reaction analysis requires an RXN drawing and molecule")
    if request.get("prepared_parts") is not None and request.get("operation") != "label_reaction":
        raise ValueError("Prepared participants require reaction labeling")
    if request.get("operation") == "label_reaction" and (
        request.get("format") != "rxn"
        or prepared_reaction
        or any(
            request.get(key) is not None
            for key in ("prepared_molecule", "prepared_import", "prepared_drawing", "document")
        )
    ):
        raise ValueError("Reaction labeling requires detached RXN participants")
    prepared_import = request.get("prepared_import")
    if prepared_import is not None and (
        not isinstance(prepared_import, dict)
        or request.get("prepared_molecule") is None
        or not (
            request.get("operation") == "import"
            and request.get("format", "smiles") in ("mol", "smiles")
            and request.get("prepared_drawing") is not None
            or request.get("operation") == "layout_import"
            and request.get("format") == "smiles"
            and request.get("prepared_drawing") is None
        )
    ):
        raise ValueError("Prepared import requires a molecular file and drawing")
    local_properties = request.get("local_properties", False)
    if not isinstance(local_properties, bool):
        raise ValueError("local_properties must be a boolean")
    local_pictures = request.get("local_pictures", False)
    if not isinstance(local_pictures, bool):
        raise ValueError("local_pictures must be a boolean")
    local_mol_output = request.get("local_mol_output", False)
    if not isinstance(local_mol_output, bool):
        raise ValueError("local_mol_output must be a boolean")
    if local_mol_output and (
        request.get("operation") != "export"
        or request.get("format") != "mol"
        or request.get("prepared_molecule") is None
    ):
        raise ValueError("Local MOL output requires a prepared molecular export")
    local_drawing_output = request.get("local_drawing_output", False)
    if not isinstance(local_drawing_output, bool):
        raise ValueError("local_drawing_output must be a boolean")
    if local_drawing_output and (
        request.get("operation") != "export"
        or request.get("format") not in ("cdxml", "cdx")
        or (
            request.get("document", {}).get("atoms")
            and (
                request.get("prepared_molecule") is None or request.get("prepared_drawing") is None
            )
        )
    ):
        raise ValueError("Local drawing output requires a prepared drawing export")
    picture_exports = request.get("picture_exports", {}) if local_pictures else None
    if local_pictures and not isinstance(picture_exports, dict):
        raise ValueError("Prepared picture exports must be an object")
    analyzer = partial(analyze, local_properties=local_properties)
    operation = request["operation"]
    response = {"engine_version": rdBase.rdkitVersion, "warnings": []}
    if operation == "label_reaction":
        return prepared.label_reaction(request.get("prepared_parts"))
    if prepared_reaction:
        doc = request["document"]
        reactions.validate(doc)
        mol = prepared.restore(request["prepared_molecule"], doc)
        check_supported(mol)
        response.update(document=None, analysis=analyzer(mol))
        return response
    if operation == "layout_import":
        if prepared_import is None:
            raise ValueError("Import layout requires a prepared molecule")
        mol = prepared.restore(request["prepared_molecule"], file=prepared_import)
        check_supported(mol)
        rdDepictor.Compute2DCoords(mol)
        conf = mol.GetConformer()
        return dict(
            rdkit_version=rdBase.rdkitVersion,
            positions=[
                dict(x=p.x, y=p.y, z=p.z)
                for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
            ],
        )
    if operation == "import" and request.get("format") in ("rxn", "rsmi"):
        doc = reactions.import_reaction(
            request.get("text", ""), request["format"], to_document, check_supported
        )
        response.update(document=doc, analysis=analyzer(from_document(doc)))
        return response
    if operation == "export" and request.get("format") in ("rxn", "rsmi"):
        response["output"] = reactions.export(
            request["document"], request.get("selected_ids"), request["format"], from_document
        )
        response["warnings"].append(
            "Reaction files preserve participants, atom maps and stereo. Save .reshiki to retain captions, arrow appearance and drawing layout."
        )
        return response
    if operation == "aromatic":
        if (data := request.get("prepared_aromatic")) is not None:
            before = prepared.restore(data["before"], request["document"])
            after = prepared.restore(data["after"], request["document"])
            check_supported(before)
            check_supported(after)
            response.update(
                document=None,
                analysis=analyzer(after),
                aromatic_identity=dict(
                    rdkit_version=rdBase.rdkitVersion,
                    before=Chem.MolToSmiles(before),
                    after=Chem.MolToSmiles(after),
                ),
            )
            return response
        result, mol = aromatic.toggle(
            request["document"], request.get("selected_ids"), from_document, to_document
        )
        response.update(document=result, analysis=analyzer(mol))
        return response
    if operation == "abbreviate":
        doc = request["document"]
        selection = request.get("selected_ids", [])
        mol = from_document(doc)
        if request.get("format") == "replace":
            result = abbreviations.replace(doc, selection, request.get("text"), to_document)
        else:
            result = abbreviations.find(doc, mol, selection, request.get("text"))
        checked = from_document(result)
        response.update(document=to_document(checked, result), analysis=analyzer(checked))
        return response
    if operation == "import":
        if prepared_import is not None:
            mol = prepared.restore(request["prepared_molecule"], file=prepared_import)
            drawing = prepared.restore(request["prepared_drawing"], file=prepared_import)
            check_supported(mol)
            check_supported(drawing)
            response.update(
                document=None,
                drawing_labels=prepared.label_drawing(drawing),
                analysis=analyzer(mol) if mol.GetNumAtoms() else None,
            )
            return response
        fmt, text = request.get("format", "smiles"), request.get("text", "")
        if not text.strip():
            raise ValueError("Enter a structure first")
        base = None
        if fmt == "smiles":
            mol = Chem.MolFromSmiles(text)
        elif fmt == "mol":
            mol = Chem.MolFromMolBlock(text, removeHs=False)
        elif fmt == "inchi":
            mol = Chem.MolFromInchi(text, removeHs=False)
        elif fmt == "cdx":
            if len(text) > (CDX_LIMIT + 2) // 3 * 4:
                raise ValueError("Drawing exceeds the 16 MB structure limit")
            mol, base = import_cdxml(
                from_cdx(base64.b64decode(text, validate=True)), local_pictures=local_pictures
            )
        elif fmt == "cdxml":
            mol, base = import_cdxml(text, local_pictures=local_pictures)
        else:
            raise ValueError("Unsupported import format")
        if mol is None:
            raise ValueError("Could not parse this structure")
        check_supported(mol)
        if fmt in ("smiles", "inchi") or not mol.GetNumConformers():
            rdDepictor.Compute2DCoords(mol)
        response.update(
            document=to_document(mol, base), analysis=analyzer(mol) if mol.GetNumAtoms() else None
        )
    elif operation in ("analyze", "clean", "export"):
        doc = request["document"]
        if not doc["atoms"]:
            if operation == "export" and request.get("format") in ("cdxml", "cdx"):
                if local_drawing_output:
                    response.update(document=doc, analysis=None, output=None)
                    return response
                output = export_cdxml(
                    doc,
                    request.get("text_layout"),
                    request.get("graphic_paths"),
                    request.get("graphic_parts"),
                    request.get("atom_indicators"),
                    picture_exports,
                )
                if request.get("format") == "cdx":
                    output = base64.b64encode(to_cdx(output)).decode("ascii")
                response.update(document=doc, analysis=None, output=output)
                return response
            raise ValueError("Draw or import a molecule first")
        if operation == "clean":
            response.update(
                cleanup.clean(
                    doc,
                    request.get("cleanup"),
                    request.get("selected_ids"),
                    from_document,
                    to_document,
                    analyzer,
                    SCALE,
                    drawing_styles.checked(doc.get("drawing_style"))["bond_length_world"],
                )
            )
            return response
        prepared_molecule = request.get("prepared_molecule")
        if prepared_molecule is not None:
            if (
                operation != "analyze"
                and request.get("format") not in ("mol", "smiles", "inchi")
                and not local_drawing_output
            ):
                raise ValueError("Prepared molecules are not supported for this operation")
            mol = prepared.restore(prepared_molecule, doc)
            check_supported(mol)
        else:
            mol = from_document(doc)
        prepared_drawing = request.get("prepared_drawing")
        if prepared_drawing is not None:
            if prepared_molecule is None:
                raise ValueError("A prepared drawing requires its prepared molecule")
            drawing = prepared.restore(prepared_drawing, doc)
            check_supported(drawing)
            response["drawing_labels"] = prepared.label_drawing(drawing)
            drawing_bonds = prepared_drawing["state"]["graph"]["bonds"]
            result_doc = None
        else:
            result_doc = to_document(mol, doc)
            drawing_bonds = result_doc["bonds"]
        response.update(document=result_doc, analysis=analyzer(mol))
        if operation == "export":
            fmt = request["format"]
            if doc.get("reactions"):
                response["warnings"].append(
                    "This drawing or molecule format does not retain reaction roles. Use RXN/reaction SMILES for reaction data, or .reshiki for the complete scheme."
                )
            exotic = {b["order"] for b in drawing_bonds} & {0, 5, 6, 7}
            if (
                (fmt == "mol" and exotic & {0, 6, 7})
                or (fmt == "smiles" and exotic & {0, 7})
                or (fmt == "inchi" and exotic)
            ):
                raise ValueError(
                    "This export cannot preserve the hydrogen, partial, dative or quadruple bonds in this drawing; use native or CDXML"
                )
            if fmt == "mol":
                response["output"] = None if local_mol_output else Chem.MolToMolBlock(mol)
            elif fmt == "smiles":
                response["output"] = Chem.MolToSmiles(mol)
            elif fmt == "inchi":
                response["output"] = Chem.MolToInchi(mol)
            elif fmt in ("cdxml", "cdx"):
                if local_drawing_output:
                    response["output"] = None
                else:
                    output = export_cdxml(
                        result_doc,
                        request.get("text_layout"),
                        request.get("graphic_paths"),
                        request.get("graphic_parts"),
                        request.get("atom_indicators"),
                        picture_exports,
                    )
                    response["output"] = (
                        base64.b64encode(to_cdx(output)).decode("ascii") if fmt == "cdx" else output
                    )
            else:
                raise ValueError("Unsupported export format")
    else:
        raise ValueError("Unknown chemistry operation")
    return response


def main():
    for line in sys.stdin:
        request = {}
        try:
            request = json.loads(line)
            if not isinstance(request, dict):
                request = {}
                raise ValueError("Request must be a JSON object")
            response = {"id": request.get("id"), "ok": True, "result": handle(request)}
        except Exception as error:
            response = {"id": request.get("id"), "ok": False, "error": str(error)}
        print(json.dumps(response, allow_nan=False), flush=True)


if __name__ == "__main__":
    main()
