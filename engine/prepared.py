"""Transport a Rust-prepared molecule to the remaining native operations.

This adapter does not sanitize or assign stereochemistry. RDKit's public API
still initializes its own valence and ring caches; check those against Rust.
"""

import math

from rdkit import Chem, rdBase

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


def restore(data, document):
    if data["rdkit_version"] != rdBase.rdkitVersion:
        raise ValueError("Prepared molecule reference version mismatch")
    state = data["state"]
    atoms, bonds = state["graph"]["atoms"], state["graph"]["bonds"]
    metadata, properties = state["metadata"], state["properties"]
    n, m = len(atoms), len(bonds)
    if (
        n > 100_000
        or m > 300_000
        or data["ids"] != [a["id"] for a in document["atoms"]]
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
                document["bonds"],
            )
        )
        or metadata["groups"]
        or state["rings"]["kind"] != "symmetric"
    ):
        raise ValueError("Invalid prepared molecule dimensions or metadata")
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
            a.SetAtomMapNum(meta["map_number"])
        a.SetChiralTag(Chem.ChiralType.values[meta["chiral_tag"]])
        a.SetHybridization(Chem.HybridizationType.names[state["hybridizations"][i]])
        # Drawing preparation has no enhanced groups or ring-stereo annotations.
        # Reject unexpected fields instead of silently discarding stereo state.
        if meta["ring_stereo"] or properties["atoms"][i]["ring_members"] is not None:
            raise ValueError("Unsupported prepared ring-stereo annotation")
        if meta.get("chiral_permutation") is not None:
            a.SetUnsignedProp("_chiralPermutation", meta["chiral_permutation"])
        p = properties["atoms"][i]
        if p["cip_code"] is not None:
            a.SetProp("_CIPCode", p["cip_code"])
        if p["cip_rank"] is not None:
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
    conformer.Set3D(False)
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
            mol.SetBoolProp(name, properties[key])
    return mol
