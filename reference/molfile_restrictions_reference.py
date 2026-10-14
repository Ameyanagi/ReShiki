"""Derive bounded generic-import contracts from explicit CTAB records.

This module does not call the Rust parser, inspect its errors, or use case names.
The producers retain the complete original result and attach these contracts only
after original success. Consumers must also require the exact current rejection.
Unsupported/malformed framing outside this small record reader gets no exemption.
"""

import re
from dataclasses import dataclass, field

KINDS = {
    "SRU",
    "MON",
    "COP",
    "CRO",
    "GRA",
    "MOD",
    "MER",
    "ANY",
    "COM",
    "MIX",
    "FOR",
    "SUP",
    "MUL",
    "DAT",
    "GEN",
}
NON_POLYMERS = {"SUP", "DAT"}
PROPERTIES = re.compile(r'([A-Z][A-Z0-9_]*)=(\([^)]*\)|"(?:""|[^"])*"|\S*)')


@dataclass
class Group:
    kind: str
    atoms: list[int] = field(default_factory=list)
    bonds: list[int] = field(default_factory=list)
    heads: list[int] = field(default_factory=list)
    properties: dict[str, str] = field(default_factory=dict)


def unsupported(category, reason):
    return dict(category=category, error="MOL input contains unsupported chemistry: " + reason)


def invalid(category, line, reason):
    return dict(category=category, error=f"Invalid MOL input at line {line}: {reason}")


def integer(text):
    return int(text.strip(" ") or "0")


def indices(value):
    if not (value.startswith("(") and value.endswith(")")):
        raise ValueError("Not a CTAB index list")
    parts = [int(item) for item in value[1:-1].split()]
    if not parts or parts[0] < 0 or parts[0] != len(parts) - 1:
        raise ValueError("Invalid CTAB index count")
    return parts[1:]


def finish(groups, bonds, reaction, generic_before_groups=False):
    if reaction and generic_before_groups:
        return unsupported(
            "rxn_attachment_targets", "RXN attachment targets; use CDXML or native ReShiki"
        )
    has_repeats = False
    for _, group in sorted(groups.items()):
        if group.kind in NON_POLYMERS:
            continue
        if group.kind == "ANY":
            return unsupported("unretained_substance_group_type", "unretained substance-group type")
        if not group.atoms:
            return unsupported(
                "empty_polymer_membership", "polymer substance group without member atoms"
            )
        membership = set(group.atoms)
        crossings = {idx for idx, (a, b) in bonds.items() if (a in membership) != (b in membership)}
        if crossings != set(group.bonds):
            return unsupported(
                "polymer_crossing_membership",
                "polymer crossing-bond list does not match its atom membership",
            )
        has_repeats = True
    if reaction and has_repeats:
        return unsupported(
            "generic_rxn_substance_groups",
            "generic RXN substance groups; use CDXML or native ReShiki",
        )
    return None


def v2000(lines, start, reaction, capture=None):
    header = lines[start + 3]
    atom_count, bond_count = integer(header[:3]), integer(header[3:6])
    if atom_count < 0 or bond_count < 0:
        raise ValueError("Negative CTAB count")
    bonds = {}
    for idx in range(bond_count):
        row = lines[start + 4 + atom_count + idx]
        bonds[idx + 1] = integer(row[:3]), integer(row[3:6])
    groups, alternatives, generic = {}, [], False
    cursor = start + 4 + atom_count + bond_count
    while cursor < len(lines):
        row, number = lines[cursor], cursor + 1
        cursor += 1
        if row.startswith("M  END"):
            if capture is not None:
                capture.update(
                    groups=groups,
                    bonds=bonds,
                    alternatives=alternatives,
                    atom_ids={idx: idx for idx in range(1, atom_count + 1)},
                )
            return finish(groups, bonds, reaction, generic), cursor
        if row.startswith(("A", "G")):
            cursor += 1  # The following alias text may itself be $MOL or M END.
            continue
        if row.startswith("S  SKP"):
            cursor += integer(row[6:9])
            continue
        kind = row[:6]
        if kind == "M  ALS":
            atom, count = integer(row[7:10]), integer(row[10:13])
            if 1 <= atom <= atom_count and row[14:15] in {"T", "F"}:
                if count == 0:
                    return invalid(
                        "empty_atom_list", number, "An atom list cannot be empty"
                    ), cursor
                generic = count > 0 or generic
                if count > 0:
                    symbols = [row[16 + 4 * idx : 20 + 4 * idx].strip() for idx in range(count)]
                    alternatives.append(dict(atom=atom, symbols=symbols, excluded=row[14] == "T"))
        elif kind == "M  LIN":
            return unsupported(
                "v2000_link_nodes", "V2000 link nodes; use V3000 LINKNODE, CDXML or native ReShiki"
            ), cursor
        elif kind == "M  STY":
            for item in range(integer(row[6:9])):
                offset = 9 + item * 8
                idx, group_kind = integer(row[offset : offset + 4]), row[offset + 5 : offset + 8]
                if group_kind not in KINDS:
                    raise ValueError("Unknown group type")
                groups.setdefault(idx, Group(group_kind))
        elif kind in {"M  SAL", "M  SBL", "M  SPA"}:
            group = groups.get(integer(row[6:10]))
            if group is None:
                continue
            if kind == "M  SPA" and group.kind not in NON_POLYMERS:
                return unsupported("polymer_parent_atoms", "polymer parent-atom membership"), cursor
            members = [integer(row[13 + i * 4 : 17 + i * 4]) for i in range(integer(row[10:13]))]
            if kind == "M  SAL":
                if any(not 1 <= idx <= atom_count for idx in members):
                    raise ValueError("Missing group atom")
                group.atoms.extend(members)
            elif kind == "M  SBL":
                if any(idx not in bonds for idx in members):
                    raise ValueError("Missing group bond")
                group.bonds.extend(members)
        elif kind == "M  SMT":
            group = groups.get(integer(row[6:10]))
            if group is not None:
                group.properties["LABEL"] = row[11:].rstrip()
        elif kind in {"M  SST", "M  SCN"}:
            for item in range(integer(row[6:9])):
                offset = 9 + item * 8
                group = groups.get(integer(row[offset : offset + 4]))
                if group is not None:
                    key = "SUBTYPE" if kind == "M  SST" else "CONNECT"
                    group.properties[key] = row[offset + 5 : offset + 8].rstrip()
        elif kind in {"M  SPL", "M  SNC"}:
            for item in range(integer(row[6:9])):
                offset = 9 + item * 8
                if len(row) < offset + 8:
                    raise ValueError("Truncated relationship")
                group = groups.get(integer(row[offset : offset + 4]))
                if group is not None and group.kind not in NON_POLYMERS:
                    return unsupported(
                        "polymer_parent_or_component_record",
                        "polymer parent or component group record",
                    ), cursor
    raise ValueError("Missing M END")


def logical_v3000(lines, cursor):
    text = ""
    while True:
        row = lines[cursor]
        cursor += 1
        if not row.startswith("M  V30 "):
            raise ValueError("Missing V3000 record")
        content = row[7:].rstrip()
        continuation = content.endswith("-")
        text += content[:-1] if continuation else content
        if not continuation:
            return text, cursor


def v3000(lines, start, reaction, capture=None):
    row, cursor = logical_v3000(lines, start)
    if row != "BEGIN CTAB":
        raise ValueError("Missing CTAB")
    row, cursor = logical_v3000(lines, cursor)
    if not row.startswith("COUNTS "):
        raise ValueError("Missing CTAB counts")
    atoms, bonds, groups = {}, {}, {}
    section, defaults, generic = None, [], False
    while True:
        row, cursor = logical_v3000(lines, cursor)
        if row == "END CTAB":
            if capture is not None:
                capture.update(groups=groups, bonds=bonds, alternatives=[], atom_ids=atoms)
            return finish(groups, bonds, reaction, generic), cursor
        if row.startswith("BEGIN "):
            section = row[6:]
            continue
        if row.startswith("END "):
            section = None
            continue
        if row.startswith("LINKNODE "):
            generic = True
        elif section == "ATOM":
            tokens = row.split()
            atoms[int(tokens[0])] = len(atoms) + 1
            generic = generic or tokens[1].startswith(("[", "NOT["))
        elif section == "BOND":
            tokens = row.split()
            bonds[int(tokens[0])] = int(tokens[2]), int(tokens[3])
            keys = {key for key, _ in PROPERTIES.findall(row)}
            generic = generic or {"ENDPTS", "ATTACH"}.issubset(keys)
        elif section == "SGROUP":
            if row.startswith("DEFAULT "):
                defaults = PROPERTIES.findall(row[8:])
                continue
            tokens = row.split(maxsplit=3)
            idx, kind = int(tokens[0]), tokens[1]
            if kind not in KINDS or idx in groups:
                raise ValueError("Unknown or duplicate group")
            group = Group(kind)
            explicit = PROPERTIES.findall(tokens[3] if len(tokens) > 3 else "")
            overridden = {key for key, _ in explicit}
            for key, value in explicit + [(k, v) for k, v in defaults if k not in overridden]:
                group.properties[key] = value
                if key == "PATOMS" and kind not in NON_POLYMERS:
                    return unsupported(
                        "polymer_parent_atoms", "polymer parent-atom membership"
                    ), cursor
                if key == "PARENT" and kind not in NON_POLYMERS:
                    return unsupported(
                        "polymer_parent_group", "nested polymer parent groups"
                    ), cursor
                if key == "COMPNO" and kind not in NON_POLYMERS:
                    return unsupported(
                        "polymer_component_number", "polymer component numbers"
                    ), cursor
                if key == "XBCORR":
                    return unsupported(
                        "polymer_crossing_correspondence", "polymer crossing-bond correspondence"
                    ), cursor
                if key == "MULT" and kind != "MUL":
                    return unsupported(
                        "non_multiple_multiplicity",
                        "multiplicity on a non-multiple substance group",
                    ), cursor
                if key == "ATOMS":
                    members = indices(value)
                    if any(member not in atoms for member in members):
                        raise ValueError("Missing group atom")
                    group.atoms.extend(members)
                elif key in {"XBONDS", "CBONDS", "XBHEAD"}:
                    members = indices(value)
                    if any(member not in bonds for member in members):
                        if key == "XBHEAD":
                            return invalid(
                                "missing_head_bond_reference", cursor, "Missing group bond bookmark"
                            ), cursor
                        raise ValueError("Missing group bond")
                    (group.heads if key == "XBHEAD" else group.bonds).extend(members)
            groups[idx] = group


def molecule(lines, start, reaction, capture=None):
    if "V3000" in lines[start + 3]:
        restriction, cursor = v3000(lines, start + 4, reaction, capture)
        if restriction is None:
            if not lines[cursor].startswith("M  END"):
                raise ValueError("Missing M END")
            cursor += 1
        return restriction, cursor
    return v2000(lines, start, reaction, capture)


def input_generic(text):
    """Return supported group/alternative records with source bookmark bindings."""
    lines = [row.removesuffix("\r") for row in text.split("\n")]
    capture = {}
    try:
        if molecule(lines, 0, False, capture)[0] is not None:
            return None
        if capture.get("alternatives") or any(
            group.kind not in NON_POLYMERS for group in capture.get("groups", {}).values()
        ):
            return capture
    except (IndexError, KeyError, ValueError):
        pass
    return None


def generic_restriction(text, *, reaction=False):
    """Return the exact first representability restriction, or no allowance."""
    # Rust str::lines splits LF/CRLF; Python splitlines also splits Unicode
    # separators and would produce incorrect physical error-line contracts.
    lines = [row.removesuffix("\r") for row in text.split("\n")]
    try:
        if not reaction:
            return molecule(lines, 0, False)[0]
        if not lines[0].startswith("$RXN"):
            return None
        if lines[0][5:10] == "V3000":
            counts, cursor = logical_v3000(lines, 4)
            fields = counts.split()
            if fields[0].upper() != "COUNTS":
                return None
            counts = [int(value) for value in fields[1:4]]
            counts += [0] * (3 - len(counts))
            for count, role in zip(counts, ("REACTANT", "PRODUCT", "AGENT"), strict=True):
                if count or role != "AGENT":
                    marker, cursor = logical_v3000(lines, cursor)
                    if marker != "BEGIN " + role:
                        return None
                for _ in range(count):
                    restriction, cursor = v3000(lines, cursor, True)
                    if restriction is not None:
                        return restriction
                if count or role != "AGENT":
                    marker, cursor = logical_v3000(lines, cursor)
                    if marker != "END " + role:
                        return None
        else:
            counts = [integer(lines[4][offset : offset + 3]) for offset in (0, 3, 6)]
            cursor = 5
            for _ in range(sum(counts)):
                if not lines[cursor].startswith("$MOL"):
                    return None
                restriction, cursor = molecule(lines, cursor + 1, True)
                if restriction is not None:
                    return restriction
    except (IndexError, KeyError, ValueError):
        return None
    return None
