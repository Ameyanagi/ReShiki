"""Independently retain the represented CTAB generic subset in expected drawings."""

import copy
import re
import struct
from typing import TYPE_CHECKING

from rdkit import Chem, rdBase
from rdkit.Chem import rdCIPLabeler

from engine.worker import to_document

if TYPE_CHECKING or __package__:
    from .molfile_restrictions_reference import NON_POLYMERS, input_generic
    from .perception_reference import snapshot
    from .valence_reference import ORDERS
else:
    from molfile_restrictions_reference import NON_POLYMERS, input_generic
    from perception_reference import snapshot
    from valence_reference import ORDERS

NOTICE = (
    "Generic choices retained. Expand generic structures to choose concrete atoms, "
    "attachment positions or finite repeats before calculating identifiers or molecular properties."
)
USAGES = {
    "SRU": "SRU",
    "MON": "Monomer",
    "MER": "Mer",
    "COP": "Copolymer",
    "CRO": "Crosslink",
    "GRA": "Graft",
    "MOD": "Modification",
    "MUL": "MultipleGroup",
    "MIX": "MixtureUnordered",
    "FOR": "MixtureOrdered",
    "COM": "Component",
    "GEN": "Generic",
}


def f32(value):
    return struct.unpack("f", struct.pack("f", value))[0]


def string(value):
    return (
        value[1:-1].replace('""', '"') if value.startswith('"') and value.endswith('"') else value
    )


def count(label):
    label = label.strip()
    if re.fullmatch(r"\+?[0-9]+", label):
        lo = hi = int(label)
    elif match := re.fullmatch(r"([0-9]+)\s*-\s*([0-9]+)", label):
        lo, hi = (int(part) for part in match.groups())
    else:
        if not label or len(label.encode()) > 32 or any(ord(char) < 32 for char in label):
            raise ValueError("Invalid represented repeat label")
        return dict(kind="indefinite", label=label)
    if not 1 <= lo <= hi <= 1000:
        raise ValueError("Invalid represented repeat count")
    return dict(kind="finite", min=lo, max=hi)


def bracket(idx, positions):
    # Drawing storage rounds source coordinates to f32 before member bounds
    # are measured. Round each arithmetic operation, preserving exact geometry.
    lo_x, lo_y = (min(f32(p[key]) for p in positions) for key in ("x", "y"))
    hi_x, hi_y = (max(f32(p[key]) for p in positions) for key in ("x", "y"))
    start = dict(x=f32(lo_x - 14), y=f32(lo_y - 24))
    end = dict(x=f32(hi_x + 14), y=f32(hi_y + 24))
    dx, dy = f32(end["x"] - start["x"]), f32(end["y"] - start["y"])
    return dict(
        id=idx,
        kind="brackets",
        origin=dict(
            x=min(start["x"], f32(start["x"] + dx)), y=min(start["y"], f32(start["y"] + dy))
        ),
        axis_x=dict(x=max(abs(dx), 1.0), y=0.0),
        axis_y=dict(x=0.0, y=max(abs(dy), 1.0)),
        style=dict(stroke=[0, 0, 0], fill=None, width_pt=f32(0.6), pattern="solid"),
        sides="both",
        phase="solid",
        phase_flipped=False,
        layer=-1,
    )


def atom_display(label):
    style = dict(
        family="Arial",
        size_pt=7.5,
        bold=False,
        italic=True,
        underline=False,
        color=[0, 0, 0],
        script="normal",
        formula=False,
    )
    return dict(
        variable=label,
        carbons=None,
        hydrogens=None,
        hydrogen_position="auto",
        number=None,
        stereo=dict(show=None, offset=None, style=style),
    )


def generic_drawing_addition(text, document):
    """Add only raw-input generic fields and deterministic member-bound brackets.

    All old atoms, bonds, labels and objects are retained in the metadata guard.
    This never inspects the current drawing and refuses a preexisting generic.
    """
    source = input_generic(text)
    if source is None:
        return None
    if document.get("generic"):
        raise ValueError("Original drawing unexpectedly already contains generic definitions")
    expected = copy.deepcopy(document)
    atoms = {atom["id"]: atom for atom in expected["atoms"]}
    stable = source["atom_ids"]
    if set(atoms) != set(stable.values()):
        raise ValueError("Original drawing atom IDs differ from source CTAB")
    generic, graphics = {}, []
    alternatives = []
    table = Chem.GetPeriodicTable()
    for site in source["alternatives"]:
        choices = []
        for symbol in site["symbols"]:
            isotope = {"D": 2, "T": 3}.get(symbol, 0)
            symbol = "H" if isotope else symbol
            if table.GetAtomicNumber(symbol) == 0:
                raise ValueError("Non-element alternative")
            choices.append(dict(kind="element", symbol=symbol, isotope=isotope))
        if not choices or len({(c["symbol"], c["isotope"]) for c in choices}) != len(choices):
            raise ValueError("Invalid or duplicate element choices")
        idx = stable[site["atom"]]
        alternatives.append(dict(atom=idx, choices=choices, excluded=site["excluded"]))
        atom = atoms[idx]
        if atom["element"] != "*" or atom.get("display"):
            raise ValueError("Original alternative atom must be an undecorated dummy")
        atom["no_implicit"] = True
        label = "[" + ("NOT " if site["excluded"] else "") + ",".join(site["symbols"]) + "]"
        atom["display"] = atom_display(label)
    if alternatives:
        generic["alternatives"] = alternatives
    repeats = []
    identifiers = [a["id"] for a in expected["atoms"]]
    for field in ("graphics", "annotations", "arrows", "groups"):
        identifiers.extend(obj["id"] for obj in expected.get(field, []) if "id" in obj)
    next_id = max(identifiers, default=0) + 1
    for _, group in sorted(source["groups"].items()):
        if group.kind in NON_POLYMERS:
            continue
        if group.kind not in USAGES:
            raise ValueError("Unrepresented repeat group")
        props = {key: string(value) for key, value in group.properties.items()}
        label = props.get("LABEL", props.get("MULT", "n"))
        usage = USAGES[group.kind]
        if group.kind == "COP":
            usage = {
                "ALT": "CopolymerAlternating",
                "RAN": "CopolymerRandom",
                "BLO": "CopolymerBlock",
            }.get(props.get("SUBTYPE"), usage)
        membership = set(group.atoms)
        ordered = list(dict.fromkeys(group.heads + group.bonds))
        crossings = []
        for idx in ordered:
            a, b = source["bonds"][idx]
            inside, outside = (a, b) if a in membership else (b, a)
            crossings.append(dict(inside=stable[inside], outside=stable[outside]))
        members = [stable[idx] for idx in group.atoms]
        if not members or len(set(members)) != len(members):
            raise ValueError("Invalid repeat members")
        repeats.append(
            dict(
                bracket=next_id,
                members=members,
                crossings=crossings,
                usage=usage,
                pattern={"HH": "head_to_head", "HT": "head_to_tail"}.get(
                    props.get("CONNECT"), "either_unknown"
                ),
                count=count(props.get("MULT", label)),
                label=label,
                flip="Unspecified",
                **(dict(head_count=len(group.heads)) if group.heads else {}),
            )
        )
        graphics.append(bracket(next_id, [atoms[idx]["position"] for idx in members]))
        next_id += 1
    if repeats:
        generic["repeats"] = repeats
    if not generic:
        return None
    expected["generic"] = generic
    expected["graphics"].extend(graphics)
    return dict(
        legacy_document=copy.deepcopy(document),
        document=expected,
        generic=generic,
        graphics=graphics,
        analysis=None,
        warnings=[NOTICE],
    )


def generic_molecule_extension(text):
    """Unwrap only explicit supported element-list query atoms in native RDKit."""
    lines = text.split("\n")
    if len(lines) > 3 and lines[3].rstrip().endswith("V2000"):
        # Numeric legacy atom-list records are unsupported even when a modern
        # M ALS repeats the list. A nonzero obsolete header count alone does
        # not imply such a record (modern-only files can retain that count).
        records = 4 + int(lines[3][:3]) + int(lines[3][3:6])
        if records < len(lines) and lines[records].lstrip()[:1].isdigit():
            return None
    source = input_generic(text)
    if source is None or not source["alternatives"]:
        return None
    mol = Chem.MolFromMolBlock(text, removeHs=False, strictParsing=True)
    if mol is None:
        return None
    sites = {source["atom_ids"][site["atom"]] - 1 for site in source["alternatives"]}
    if {atom.GetIdx() for atom in mol.GetAtoms() if atom.HasQuery()} != sites:
        return None
    if mol.GetStereoGroups() or any(
        bond.HasQuery() or bond.GetBondType() not in ORDERS.values() for bond in mol.GetBonds()
    ):
        return None
    work = Chem.RWMol(mol)
    for atom in list(work.GetAtoms()):
        if atom.GetIdx() in sites:
            if atom.GetAtomicNum() != 0:
                return None
            work.ReplaceAtom(atom.GetIdx(), Chem.Atom(atom), preserveProps=True)
    Chem.SanitizeMol(work)
    Chem.AssignStereochemistry(work, cleanIt=True, force=True, flagPossibleStereoCenters=True)
    return Chem.Mol(work)


def molecular_snapshot(mol):
    conf = mol.GetConformer()
    return dict(
        state=snapshot(mol, "symmetric"),
        is_3d=conf.Is3D(),
        attachment_points=[
            a.GetIntProp("molAttchpt") if a.HasProp("molAttchpt") else None for a in mol.GetAtoms()
        ],
        dummy_labels=[
            a.GetProp("dummyLabel") if a.HasProp("dummyLabel") else None for a in mol.GetAtoms()
        ],
        positions=[
            dict(x=p.x, y=p.y, z=p.z)
            for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
        ],
    )


def drawing_snapshot(mol):
    work = Chem.Mol(mol)
    Chem.Kekulize(work, clearAromaticFlags=True)
    Chem.WedgeMolBonds(work, work.GetConformer())
    for obj in list(work.GetAtoms()) + list(work.GetBonds()):
        if obj.HasProp("_CIPCode"):
            obj.ClearProp("_CIPCode")
    expected = snapshot(work, "symmetric")
    rdCIPLabeler.AssignCIPLabels(work, maxRecursiveIterations=1_250_000)
    labels = dict(
        rdkit_version=rdBase.rdkitVersion,
        atoms=[a.GetProp("_CIPCode") if a.HasProp("_CIPCode") else None for a in work.GetAtoms()],
        bonds=[
            dict(
                code=b.GetProp("_CIPCode") if b.HasProp("_CIPCode") else None,
                stereo=int(b.GetStereo()),
                stereo_atoms=list(b.GetStereoAtoms()),
            )
            for b in work.GetBonds()
        ],
    )
    return dict(expected=expected, labels=labels, document=to_document(mol))


def generic_engine_extension(text, failure):
    if failure != "Query atoms are not supported yet; import was cancelled.":
        return None
    mol = generic_molecule_extension(text)
    if mol is None:
        return None
    addition = generic_drawing_addition(text, to_document(mol))
    if addition is None:
        return None
    return dict(
        failure=failure,
        expected=dict(
            document=addition["document"],
            analysis=None,
            output=None,
            engine_version=rdBase.rdkitVersion,
            warnings=[NOTICE],
        ),
    )
