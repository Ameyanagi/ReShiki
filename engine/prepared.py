"""Transport a Rust-prepared molecule to the remaining native operations.

This adapter does not sanitize or assign stereochemistry. RDKit's public API
still initializes its own valence and ring caches; check those against Rust.
"""

import math
import struct

from rdkit import Chem, rdBase
from rdkit.Chem import rdCIPLabeler, rdDepictor

ORDERS = {
    0: Chem.BondType.HYDROGEN,
    1: Chem.BondType.SINGLE,
    2: Chem.BondType.DOUBLE,
    3: Chem.BondType.TRIPLE,
    4: Chem.BondType.AROMATIC,
    5: Chem.BondType.DATIVE,
    6: Chem.BondType.QUADRUPLE,
    7: Chem.BondType.ONEANDAHALF,
}
DIRECTIONS = {
    "none": Chem.BondDir.NONE,
    "wedge": Chem.BondDir.BEGINWEDGE,
    "hash": Chem.BondDir.BEGINDASH,
    "up": Chem.BondDir.ENDUPRIGHT,
    "down": Chem.BondDir.ENDDOWNRIGHT,
    "either_double": Chem.BondDir.EITHERDOUBLE,
    "unknown": Chem.BondDir.UNKNOWN,
}


def _ring_annotations(mol, members):
    """Transport integer-vector properties missing from Python's atomic setters.

    This is RDKit's pinned binary format, not Python pickle. Only locally built
    molecules and bounded, validated integer lists enter the serializer.
    Layout: RDKit 2026.03.6 MolPickler.cpp and RDGeneral/StreamOps.h (BSD-3-Clause).
    """
    n = mol.GetNumAtoms()
    if len(members) != n or sum(len(v) for v in members if v is not None) > 10_000_000:
        raise ValueError("Invalid prepared ring-stereo dimensions")
    if all(v is None for v in members):
        return mol
    block = bytearray()
    name = b"_ringStereoAtoms"
    for values in members:
        if values is None:
            block.extend(b"\0\0\0")  # uint16 property count, uint8 explicit flags.
            continue
        if any(type(v) is not int or not 1 <= abs(v) <= n for v in values):
            raise ValueError("Invalid prepared ring-stereo atom")
        block.extend(struct.pack("<HI", 1, len(name)))
        block.extend(name)
        block.extend(struct.pack("<BQ", 7, len(values)))  # signed vector tag, uint64 count.
        for value in values:
            block.extend(struct.pack("<i", value))
        block.append(0)
    raw = mol.ToBinary(
        Chem.PropertyPickleOptions.AllProps | Chem.PropertyPickleOptions.CoordsAsDouble
    )
    if (
        len(raw) < 21
        or struct.unpack_from("<IIiii", raw) != (0xDEADBEEF, 0, 16, 3, 0)
        or raw[-1] != 22
    ):
        raise ValueError("Prepared molecule binary version mismatch")
    # A further BEGINATOMPROPS block extends existing properties. Keep the
    # native graph, conformers, caches and all previously transported fields.
    return Chem.Mol(
        raw[:-1] + bytes([58]) + struct.pack("<i", len(block)) + block + bytes([19, 22])
    )


def restore(data, document=None, *, file=None):
    if data["rdkit_version"] != rdBase.rdkitVersion:
        raise ValueError("Prepared molecule reference version mismatch")
    state = data["state"]
    atoms, bonds = state["graph"]["atoms"], state["graph"]["bonds"]
    metadata, properties = state["metadata"], state["properties"]
    n, m = len(atoms), len(bonds)
    if (
        n > 100_000
        or m > 300_000
        or (document is not None and data["ids"] != [a["id"] for a in document["atoms"]])
        or len(set(data["ids"])) != n
        or any(
            len(items) != n
            for items in (
                data["ids"],
                data["positions"],
                metadata["atoms"],
                properties["atoms"],
                state["valences"],
                state["hybridizations"],
            )
        )
        or any(
            len(items) != m
            for items in (
                metadata["bonds"],
                properties["bond_codes"],
                state["directions"],
                state["conjugated"],
            )
        )
        or (document is not None and len(document["bonds"]) != m)
        or metadata["groups"]
        or state["rings"]["kind"] != "symmetric"
    ):
        raise ValueError("Invalid prepared molecule dimensions or metadata")
    if file is not None and (
        type(file["is_3d"]) is not bool
        or len(file["attachment_points"]) != n
        or len(file["dummy_labels"]) != n
    ):
        raise ValueError("Invalid prepared file annotations")
    rw = Chem.RWMol()
    for i, item in enumerate(atoms):
        a = Chem.Atom(item["atomic_number"])
        a.SetFormalCharge(item["charge"])
        a.SetIsotope(item["isotope"])
        a.SetNumExplicitHs(item["explicit_hydrogens"])
        a.SetNoImplicit(item["no_implicit"])
        a.SetIsAromatic(item["aromatic"])
        a.SetNumRadicalElectrons(item["radical_electrons"])
        a.SetProp("reshiki_id", str(data["ids"][i]))
        meta = metadata["atoms"][i]
        if meta["map_present"]:
            # SetAtomMapNum(0) removes the property. An explicit :0 affects
            # canonical SMILES and must remain distinct from an absent map.
            a.SetIntProp("molAtomMapNumber", meta["map_number"])
        a.SetChiralTag(Chem.ChiralType.values[meta["chiral_tag"]])
        a.SetHybridization(Chem.HybridizationType.names[state["hybridizations"][i]])
        if meta["ring_stereo"] != (properties["atoms"][i]["ring_members"] is not None):
            raise ValueError("Inconsistent prepared ring-stereo annotation")
        if file is not None:
            if (attachment := file["attachment_points"][i]) is not None:
                if type(attachment) is not int or not -(2**31) <= attachment < 2**31:
                    raise ValueError("Invalid prepared attachment point")
                a.SetIntProp("molAttchpt", attachment)
            if (label := file["dummy_labels"][i]) is not None:
                if not isinstance(label, str) or len(label.encode("utf-8")) > 16 * 1024 * 1024:
                    raise ValueError("Invalid prepared dummy label")
                a.SetProp("dummyLabel", label)
        if meta.get("chiral_permutation") is not None:
            a.SetUnsignedProp("_chiralPermutation", meta["chiral_permutation"])
        p = properties["atoms"][i]
        if p["cip_code"] is not None:
            a.SetProp("_CIPCode", p["cip_code"])
        if p["cip_rank"] is not None:
            # Atomic setters expose values, but not computed-property flags,
            # as in RDKit's JSON importer. Use transported atoms only for
            # analysis/export/identity checks, never for topology edits.
            a.SetUnsignedProp("_CIPRank", p["cip_rank"])
        for key, name in (
            ("possible", "_ChiralityPossible"),
            ("ring_candidate", "_ringStereochemCand"),
        ):
            if p[key] is not None:
                a.SetBoolProp(name, p[key])
        if p["unknown"]:
            a.SetIntProp("_UnknownStereo", 1)
        rw.AddAtom(a)
    pairs = set()
    for i, item in enumerate(bonds):
        a, b = item["a"], item["b"]
        pair = tuple(sorted((a, b)))
        if not (0 <= a < n and 0 <= b < n) or a == b or pair in pairs:
            raise ValueError("Invalid prepared bond endpoints")
        pairs.add(pair)
        rw.AddBond(a, b, ORDERS[item["order"]])
        bond = rw.GetBondWithIdx(i)
        bond.SetIsAromatic(item["aromatic"])
        bond.SetIsConjugated(state["conjugated"][i])
        bond.SetBondDir(DIRECTIONS[state["directions"][i]])
    # Adding an aromatic-order bond can mark its endpoints aromatic even when
    # sanitization left their flags clear. Restore the supplied flags last.
    for atom, item in zip(rw.GetAtoms(), atoms, strict=True):
        atom.SetIsAromatic(item["aromatic"])
    # Stereo controls may reference bonds that occur later in the input.
    for i, meta in enumerate(metadata["bonds"]):
        b = rw.GetBondWithIdx(i)
        if meta["stereo_atoms"]:
            left, right = meta["stereo_atoms"]
            b.SetStereoAtoms(left, right)
        b.SetStereo(Chem.BondStereo.values[meta["stereo"]])
        if meta["unknown_stereo"]:
            b.SetIntProp("_UnknownStereo", 1)
        if (code := properties["bond_codes"][i]) is not None:
            b.SetProp("_CIPCode", code)
    mol = rw.GetMol()
    conformer = Chem.Conformer(n)
    conformer.Set3D(file["is_3d"] if file is not None else False)
    for i, p in enumerate(data["positions"]):
        xyz = (p["x"], p["y"], p["z"])
        if not all(math.isfinite(v) for v in xyz):
            raise ValueError("Invalid prepared coordinates")
        conformer.SetAtomPosition(i, xyz)
    mol.AddConformer(conformer)
    mol.UpdatePropertyCache(strict=False)
    Chem.GetSymmSSSR(mol)
    actual = [
        dict(
            explicit_valence=a.GetValence(Chem.ValenceType.EXPLICIT),
            implicit_hydrogens=a.GetNumImplicitHs(),
        )
        for a in mol.GetAtoms()
    ]
    if (
        actual != state["valences"]
        or [list(r) for r in mol.GetRingInfo().AtomRings()] != state["rings"]["atoms"]
    ):
        raise ValueError("Prepared molecule cache mismatch")
    for key, name in (("done", "_StereochemDone"), ("needs_detection", "_needsDetectBondStereo")):
        if properties[key] is not None:
            mol.SetIntProp(name, int(properties[key]), computed=key == "done")
    return _ring_annotations(mol, [p["ring_members"] for p in properties["atoms"]])


def _reaction_parts(parts, minimum, *, layout=False):
    if not isinstance(parts, list) or not minimum <= len(parts) <= 10000:
        raise ValueError("Invalid prepared reaction participants")
    total, ids = 0, set()
    for part in parts:
        if (
            not isinstance(part, dict)
            or set(part)
            - ({"molecule", "file", "atom_properties"} if layout else {"molecule", "file"})
            or not {"molecule", "file"} <= set(part)
            or not isinstance(part["molecule"], dict)
            or not isinstance(part["file"], dict)
        ):
            raise ValueError("Invalid prepared reaction participant")
        atom_ids = part["molecule"].get("ids", [])
        total += len(atom_ids)
        if not atom_ids or total > 10000 or ids.intersection(atom_ids):
            raise ValueError("Invalid prepared reaction atom count or identities")
        ids.update(atom_ids)
    return parts


def layout_reaction(parts):
    """Only supply coordinates for participants without an input conformer."""
    positions = []
    for part in _reaction_parts(parts, 1, layout=True):
        mol = restore(part["molecule"], file=part["file"])
        _layout_properties(mol, part.get("atom_properties", [[] for _ in mol.GetAtoms()]))
        rdDepictor.Compute2DCoords(mol)
        conf = mol.GetConformer()
        positions.append(
            [
                dict(x=p.x, y=p.y, z=p.z)
                for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
            ]
        )
    return dict(rdkit_version=rdBase.rdkitVersion, positions=positions)


def _layout_properties(mol, properties):
    """CX values retain string types; native numeric conversions happen lazily."""
    if not isinstance(properties, list) or len(properties) != mol.GetNumAtoms():
        raise ValueError("Invalid layout atom property dimensions")
    size = 0
    for atom, entries in zip(mol.GetAtoms(), properties, strict=True):
        if not isinstance(entries, list):
            raise ValueError("Invalid layout atom properties")
        for entry in entries:
            if (
                not isinstance(entry, list)
                or len(entry) != 2
                or any(not isinstance(field, list) for field in entry)
            ):
                raise ValueError("Invalid layout atom property")
            size += 1 + sum(len(field) for field in entry)
            if size > 1024 * 1024 or any(
                type(value) is not int or not 0 <= value <= 255
                for field in entry
                for value in field
            ):
                raise ValueError("Invalid or excessive layout property bytes")
            name, value = (bytes(field) for field in entry)
            atom.SetProp(name, value)


def label_reaction(parts):
    """Label detached participants; their parsing and layout have already finished."""
    parts = _reaction_parts(parts, 2)
    return dict(
        rdkit_version=rdBase.rdkitVersion,
        labels=[label_drawing(restore(p["molecule"], file=p["file"])) for p in parts],
    )


def label_drawing(mol):
    """Full CIP labels, including the bond-stereo controls the native pass changes."""
    for item in list(mol.GetAtoms()) + list(mol.GetBonds()):
        if item.HasProp("_CIPCode"):
            item.ClearProp("_CIPCode")
    rdCIPLabeler.AssignCIPLabels(mol, maxRecursiveIterations=1_250_000)
    return dict(
        rdkit_version=rdBase.rdkitVersion,
        atoms=[a.GetProp("_CIPCode") if a.HasProp("_CIPCode") else None for a in mol.GetAtoms()],
        bonds=[
            dict(
                code=b.GetProp("_CIPCode") if b.HasProp("_CIPCode") else None,
                stereo=int(b.GetStereo()),
                stereo_atoms=list(b.GetStereoAtoms()),
            )
            for b in mol.GetBonds()
        ],
    )
