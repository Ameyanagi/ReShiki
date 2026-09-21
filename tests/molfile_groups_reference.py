"""Substance-group import fixtures authored with the native writer or CTAB records."""

import os
from itertools import product
from pathlib import Path

from rdkit import Chem
from rdkit.Chem import rdDepictor

KINDS = (
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
)


def v3(records, defaults="", atoms="CCO", bond_type=None):
    mol = Chem.MolFromSmiles(atoms)
    rdDepictor.Compute2DCoords(mol)
    text = Chem.MolToMolBlock(mol, forceV3000=True)
    text = text.replace(
        f"COUNTS {mol.GetNumAtoms()} {mol.GetNumBonds()} 0",
        f"COUNTS {mol.GetNumAtoms()} {mol.GetNumBonds()} {len(records)}",
    )
    if bond_type is not None:
        text = text.replace("M  V30 1 1 1 2", f"M  V30 1 {bond_type} 1 2")
    content = ["M  V30 BEGIN SGROUP"]
    if defaults:
        content.append("M  V30 DEFAULT " + defaults)
    content.extend("M  V30 " + record for record in records)
    content.append("M  V30 END SGROUP")
    return text.replace("M  V30 END CTAB", "\n".join(content) + "\nM  V30 END CTAB")


def v2(records, atoms="CCO"):
    mol = Chem.MolFromSmiles(atoms)
    rdDepictor.Compute2DCoords(mol)
    return Chem.MolToMolBlock(mol).replace("M  END", "\n".join(records) + "\nM  END")


def cases():
    for kind, version, members in product(KINDS, (False, True), ([], [0], [1, 2], [2, 1, 2])):
        mol = Chem.MolFromSmiles("CCO")
        rdDepictor.Compute2DCoords(mol)
        group = Chem.CreateMolSubstanceGroup(mol, kind)
        for atom in members:
            group.AddAtomWithIdx(atom)
        group.SetProp("LABEL", "OH")
        group.SetProp("MULT", "2")
        group.SetProp("SUBTYPE", "ALT")
        group.SetProp("CONNECT", "HT")
        yield f"native/{kind}/{version}/{members}", Chem.MolToMolBlock(mol, forceV3000=version)
    for field, data, members, version in product(
        ("ZCH", "HYD", "ZBO", "MRV_IMPLICIT_H", "MRV_COORDINATE_BOND_TYPE", "ordinary data"),
        (
            "",
            "0",
            "1",
            "2",
            "-1",
            "3;1",
            ";",
            " 1;0 ",
            "bad",
            "IMPL_H1",
            "IMPL_H-1",
            "+1",
            "1 2",
            "1-2",
            "2147483648",
            "4294967295",
            "4294967296",
            "999999999999999999999999",
            "-2147483648",
        ),
        ([], [0], [1, 2], [2, 1, 2]),
        (False, True),
    ):
        mol = Chem.MolFromSmiles("CCO")
        rdDepictor.Compute2DCoords(mol)
        group = Chem.CreateMolDataSubstanceGroup(mol, field, data)
        for atom in members:
            group.AddAtomWithIdx(atom)
        if field == "ZBO" and any(a in (0, 1) for a in members):
            group.AddBondWithIdx(0)
        yield (
            f"data/{field}/{data}/{members}/{version}",
            Chem.MolToMolBlock(mol, forceV3000=version),
        )
    for count, atom, bond, parent, kind in product(
        (0, 1, 2, 4), (0, 1, 3, 4), (0, 1, 2, 3), (0, 1, 2, 4), ("SUP", "DAT")
    ):
        props = f"ATOMS=({count} {atom}) XBONDS=(1 {bond}) PATOMS=(1 {parent})"
        yield f"v3 members/{props}/{kind}", v3([f"1 {kind} 0 {props}"])
    for prop, values in (
        ("SUBTYPE", ("ALT", "RAN", "BLO", "BAD", '""')),
        ("CONNECT", ("HH", "HT", "EU", "BAD")),
        ("CLASS", ("AA", "dAA", "CHEM", "LGRP", "BAD", '""')),
        ("COMPNO", ("0", "256", "257", "-1", "bad")),
        ("PARENT", ("0", "1", "2", "-1", "bad")),
        ("BRKXYZ", ("(9 0 0 0 1 1 1 2 2 2)", "(6 0 0 0 1 1 1)", "(10 0 0 0 1 1 1 2 2 2 3)")),
        ("CSTATE", ("(4 1 0 1 0)", "(1 1)", "(4 2 0 0 0)", "(4 0 0 0 0)")),
        ("SAP", ("(3 2 1 A)", "(3 2 AIDX A)", "(3 2 0 A)", "(3 0 1 A)", "(3 4 1 A)", "(2 2 1 A)")),
        ("XBHEAD", ("(1 1)", "(3 1 2 3)", "(1 20)")),
        ("XBCORR", ("(1 1)", "(3 1 2 3)", "(1 0)")),
        ("LABEL", ('"acid = base"', '"日本語"', '"quoted ""text"""', '""')),
    ):
        for value, kind in product(values, ("SUP", "DAT")):
            yield (
                f"v3 label/{prop}/{value}/{kind}",
                v3([f"1 {kind} 0 ATOMS=(1 2) XBONDS=(1 1) {prop}={value}"]),
            )
    for field, data, order in product(
        ("MRV_COORDINATE_BOND_TYPE", "ZCH", "HYD", "ZBO"),
        ("1", "0", "999", "bad"),
        (0, 1, 5, 6, 7, 8, 9, 15),
    ):
        yield (
            f"v3 bond conversion/{field}/{data}/{order}",
            v3(
                [f"1 DAT 0 ATOMS=(1 1) XBONDS=(1 1) FIELDNAME={field} FIELDDATA={data}"],
                bond_type=order,
                atoms="N[Cu]",
            ),
        )
    for h, symbol, val in product(("0", "1", "2", "-1", "256"), ("n", "c"), ("0", "3", "15")):
        text = v3(
            [f"1 DAT 0 ATOMS=(1 1) FIELDNAME=MRV_IMPLICIT_H FIELDDATA=IMPL_H{h}"],
            atoms=f"{symbol}1ccccc1",
        )
        # Native writer emits Kekulé bonds; author aromatic bond order explicitly.
        lines = text.splitlines()
        in_bonds = False
        for i, line in enumerate(lines):
            if line == "M  V30 BEGIN BOND":
                in_bonds = True
            elif line == "M  V30 END BOND":
                in_bonds = False
            elif in_bonds:
                fields = line.split()
                fields[3] = "4"
                lines[i] = " ".join(fields)
            elif line.startswith("M  V30 1 " + symbol.upper()):
                lines[i] += f" VAL={val}"
        yield f"aromatic H/{h}/{symbol}/{val}", "\n".join(lines) + "\n"
    for order, direction, first in product((0, 1, 5, 8, 9, 15), range(4), (False, True)):
        groups = [
            f'{1 if first else 2} DAT 0 ATOMS=(2 1 2) FIELDNAME=HYD FIELDDATA="2;2"',
            f"{2 if first else 1} DAT 0 FIELDNAME=MRV_COORDINATE_BOND_TYPE FIELDDATA=1",
        ]
        text = v3(groups, bond_type=order, atoms="N[Cu]")
        text = text.replace(f"M  V30 1 {order} 1 2\n", f"M  V30 1 {order} 1 2 CFG={direction}\n")
        yield f"coordinate H and stereo/{order}/{direction}/{first}", text
    for atom_id, bond_id in product((1, 10, 40, -1), (1, 20, -1)):
        text = v3([f"1 DAT 0 ATOMS=(1 {atom_id}) XBONDS=(1 {bond_id}) FIELDNAME=ZCH FIELDDATA=1"])
        text = text.replace("M  V30 1 C ", f"M  V30 {atom_id} C ")
        text = text.replace("M  V30 1 1 1 2", f"M  V30 {bond_id} 1 {atom_id} 2")
        yield f"bookmarks/{atom_id}/{bond_id}", text
    for query, op, data, members in product(
        ("SMARTSQ", "SQ", "unknown"),
        ("=", "!=", ""),
        ("C", "[!#6]", "invalid???", "", "CC"),
        ("(0)", "(1 1)"),
    ):
        yield (
            f"query/{query}/{op}/{data}/{members}",
            v3([f'1 DAT 0 ATOMS={members} QUERYTYPE={query} QUERYOP="{op}" FIELDDATA="{data}"']),
        )
    for records in (
        ["1 SUP 0 ATOMS=(1 1)", "1 DAT 0 ATOMS=(1 2)"],
        [
            "5 DAT 0 ATOMS=(1 1) FIELDNAME=ZCH FIELDDATA=1",
            "1 DAT 0 ATOMS=(1 1) FIELDNAME=ZCH FIELDDATA=0",
        ],
        ["1 DAT 0 ATOMS=(1 1) FIELDNAME=HYD FIELDDATA=0 FIELDDATA=2"],
        ["1 BAD 0 ATOMS=(1 1)"],
    ):
        yield f"v3 ordering/{records}", v3(records)
    for defaults in (
        "FIELDNAME=ZCH FIELDDATA=1",
        "ATOMS=(1 1) FIELDNAME=HYD FIELDDATA=2",
        'LABEL="with spaces"',
    ):
        yield (
            f"defaults/{defaults}",
            v3(["2 DAT 0 ATOMS=(1 1)", "1 DAT 0 ATOMS=(1 2) FIELDNAME=ZCH FIELDDATA=0"], defaults),
        )
    for prefix in ([], ["M  STY  1   1 SUP", "M  SAL   1  1   2", "M  SBL   1  1   1"]):
        for record in (
            "M  STY  1   1 BAD",
            "M  STY  1   1 SUP",
            "M  SAL   1  1   4",
            "M  SPA   1  1   2",
            "M  SPA   1  1   1",
            "M  SST  1   1 ALT",
            "M  SST  1   1 BAD",
            "M  SCN  1   1 HT ",
            "M  SCN  1   1 NO ",
            "M  SDS EXP  1   1",
            "M  SDS BAD  1   1",
            "M  SBT  1   1   2",
            "M  SBT  1   1   1",
            "M  SLB  1   1  12",
            "M  SPL  1   1 999",
            "M  SNC  1   1 256",
            "M  SNC  1   1 257",
            "M  SMT   1 label",
            "M  SCL   1 arbitrary",
            "M  SMT   1",
            "M  SBV   1   1    1.0000    0.0000",
            "M  SDI   1  4    0.0000    0.0000    1.0000    1.0000",
            "M  SAP   1  1   2   1 A ",
            "M  SAP   1  1   4   1 A ",
            "M  SED   1 x",
        ):
            for cut in (0, 1, 3):
                yield (
                    f"v2 syntax/{bool(prefix)}/{record}/{cut}",
                    v2([*prefix, record[: len(record) - cut]]),
                )
    for count, end in product(range(6), (True, False)):
        records = ["M  STY  1   1 DAT", "M  SAL   1  1   1", "M  SDT   1 HYD"]
        records += ["M  SCD   1 " + "1" * 69] * count
        if end:
            records.append("M  SED   1 2")
        yield f"v2 data continuations/{count}/{end}", v2(records)
    source = os.environ.get("RESHIKI_RDKIT_SOURCE")
    if source:
        root = Path(source) / "Code/GraphMol/FileParsers"
        for path in sorted((root / "sgroup_test_data").glob("*.mol")):
            yield f"source/groups/{path.name}", path.read_text(errors="replace")
        for path in sorted((root / "test_data/sgroupFragments").glob("*.sdf")):
            for i, text in enumerate(path.read_text(errors="replace").split("$$$$")):
                if text.strip():
                    yield f"source/groups/{path.name}/{i}", text.removeprefix("\n")
