"""Read MOL files through native RDKit; never import ReShiki's worker or parser."""

import json
import os
import random
from itertools import product
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase
from rdkit.Chem import rdDepictor

if TYPE_CHECKING or __package__:
    from .perception_reference import snapshot
    from .valence_reference import ORDERS
else:
    from perception_reference import snapshot
    from valence_reference import ORDERS


def emit(name, text):
    try:
        mol = Chem.MolFromMolBlock(text, removeHs=False, strictParsing=True)
        if mol is None:
            raise ValueError("Native MOL reader rejected input")
        if (
            mol.GetStereoGroups()
            or any(
                a.HasQuery() or a.GetNumRadicalElectrons() > 2 or int(a.GetChiralTag()) > 2
                for a in mol.GetAtoms()
            )
            or any(
                b.HasQuery() or b.GetBondType() not in ORDERS.values() or int(b.GetStereo()) > 5
                for b in mol.GetBonds()
            )
        ):
            raise ValueError("Outside the existing editable chemistry contract")
        conf = mol.GetConformer()
        expected = dict(
            state=snapshot(mol, "symmetric"),
            positions=[
                dict(x=p.x, y=p.y, z=p.z)
                for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
            ],
        )
        failure = None
    except (RuntimeError, ValueError, OverflowError) as error:
        expected, failure = None, str(error)
    print(json.dumps(dict(name=name, text=text, expected=expected, failure=failure)))


def native_molecules():
    for text in (
        "",
        "CCO",
        "[2H]O[3H]",
        "[13CH3:11][NH3+:22]",
        "[Cl-].[Na+]",
        "[CH3]",
        "[CH2]",
        "[O]",
        "c1ccccc1",
        "c1cc[nH]c1",
        "c1ccc2[nH]ccc2c1",
        "F/C=C/F",
        "F/C=C\\F",
        "C/C=N/O",
        "CC(O)=N",
        "F[C@](Cl)(Br)I",
        "C[C@@H](N)C(=O)O",
        "C1CC2CCC1C2",
        "c1ccc(-c2ccccc2)cc1",
        "N->[Cu]<-N",
        "O=N(=O)c1ccccc1",
        "C[S@@](=O)CC",
        "*C",
        "[R]",
        "[Fe+2]",
        "[H]N([H])[H]",
    ):
        mol = Chem.MolFromSmiles(text)
        if mol is not None:
            yield text, mol
    # The pinned wheel supplies this independent public chemistry corpus on CI.
    for i, mol in enumerate(
        Chem.SDMolSupplier(
            str(Path(RDConfig.RDDataDir) / "NCI/first_200.props.sdf"), removeHs=False
        )
    ):
        if mol is not None:
            yield f"NCI/{i}", mol
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    for template in templates:
        if template.get("smiles"):
            mol = Chem.MolFromSmiles(template["smiles"])
            if mol is not None:
                yield f"template/{template['smiles']}", mol


def atom_line(symbol="C", **fields):
    line = f"{0:10.4f}{0:10.4f}{0:10.4f} {symbol:<3}" + " 0" + "  0" * 11
    for key, value in fields.items():
        start, width = {
            "mass": (34, 2),
            "charge": (36, 3),
            "parity": (39, 3),
            "hs": (42, 3),
            "valence": (48, 3),
            "map": (60, 3),
        }[key]
        line = line[:start] + f"{value:{width}}" + line[start + width :]
    return line


def block(atoms, bonds=(), properties=()):
    return "\n".join(
        (
            "reference",
            "                    2D",
            "",
            f"{len(atoms):3}{len(bonds):3}  0  0  0  0  0  0  0  0999 V2000",
            *atoms,
            *bonds,
            *properties,
            "M  END",
            "",
        )
    )


def v3_block(atoms, bonds=(), extras=()):
    return "\n".join(
        (
            "reference",
            "                    2D",
            "",
            "  0  0  0  0  0  0  0  0  0  0999 V3000",
            "M  V30 BEGIN CTAB",
            f"M  V30 COUNTS {len(atoms)} {len(bonds)} 0 0 0",
            "M  V30 BEGIN ATOM",
            *("M  V30 " + s for s in atoms),
            "M  V30 END ATOM",
            *(
                ("M  V30 BEGIN BOND", *("M  V30 " + s for s in bonds), "M  V30 END BOND")
                if bonds
                else ()
            ),
            *extras,
            "M  V30 END CTAB",
            "M  END",
            "",
        )
    )


def syntax_cases():
    for symbol in ("R123", "R01", "R001", "R99", "R999", "R1000", "R9a", "R8a", "Rabc"):
        emit(f"v3 dummy label/{symbol}", v3_block([f"1 {symbol} 0 0 0 0"]))
    for symbol, chg, val, mass, rad in product(
        ("C", "N", "O", "Cu", "D", "R", '"CL"'),
        (-1, 0, 1),
        (-1, 0, 3, 15),
        (0, 2, 13.9),
        (0, 1, 2, 3),
    ):
        emit(
            f"v3 atom/{symbol}/{chg}/{val}/{mass}/{rad}",
            v3_block([f"42 {symbol} 1.25 -3.5 0 7 CHG={chg} VAL={val} MASS={mass} RAD={rad}"]),
        )
    for key, values in (
        ("CFG", (0, 1, 2, 3, 4)),
        ("HCOUNT", (-1, 0, 1, 2)),
        ("RBCNT", (-2, -1, 0, 1)),
        ("UNSAT", (-1, 0, 1, 2)),
        ("SUBST", (-2, -1, 0, 1, 6)),
        ("RGROUPS", ("(0)", "(1 2)", "(2 3 4)", "(3 1)")),
        ("ATTCHPT", (-1, 0, 1, 2, 3)),
        ("ATTCHORD", (0, 1)),
        ("STBOX", (0, 1)),
        ("INVRET", (0, 1, 2)),
        ("EXACHG", (0, 1)),
    ):
        for value in values:
            emit(f"v3 property/{key}/{value}", v3_block([f"1 C 0 0 0 0 {key}={value}"]))
    for order, cfg, topo in product(range(11), range(5), range(3)):
        emit(
            f"v3 bond/{order}/{cfg}/{topo}",
            v3_block(
                ["10 C 0 0 0 0", "40 N 1.5 0 0 0"], [f"93 {order} 10 40 CFG={cfg} TOPO={topo}"]
            ),
        )
    basic = v3_block(["10 C 0 0 0 0", "40 N 1.5 0 0 0"], ["93 1 10 40"])
    for i in range(len(basic)):
        emit(f"v3 truncated/{i}", basic[:i])
    for replacement in (
        "10 C 0 0 0 0 -\nM  V30 CHG=1",
        '10 C 0 0 0 0 LABEL="hello world"',
        "10 C 0 0 0 0 UNKNOWN=(3 1 2 3)",
    ):
        emit(f"v3 grammar/{replacement}", basic.replace("10 C 0 0 0 0", replacement))
    for record in (
        "A    1\n日本語",
        "V    1 value",
        "G    1\nlabel",
        "S  SKP  1\nignored",
        "M  PXA   1 label",
        "M  FOO unknown extension",
        "M  ALS   1  0 F",
        "M  RGP  0",
        "M  LIN  1   1   2   2   0",
    ):
        emit(f"v2 record/{record}", block([atom_line(), atom_line()], properties=(record,)))
    for record, values in (
        ("SUB", (-1, 0, 1)),
        ("UNS", (0, 1, 2)),
        ("RBC", (-2, 0, 1)),
        ("APO", (-1, 0, 1, 2, 3, 4)),
    ):
        for value in values:
            emit(
                f"v2 property/{record}/{value}",
                block([atom_line()], properties=(f"M  {record}  1   1{value:4}",)),
            )
    # Keep the remaining import boundaries exercised in CI, not just the optional source survey.
    for text in ("C[C@H](O)C(=O)O", "C[C@H](O)[C@H](O)C |&1:1,3|"):
        mol = Chem.MolFromSmiles(text)
        rdDepictor.Compute2DCoords(mol)
        emit("pending/enhanced stereo", Chem.MolToMolBlock(mol, forceV3000=True))
        conf = mol.GetConformer()
        conf.Set3D(True)
        p = conf.GetAtomPosition(0)
        conf.SetAtomPosition(0, (p.x, p.y, 0.5))
        emit("pending/3D", Chem.MolToMolBlock(mol, forceV3000=True))
    for direction in (Chem.BondDir.BEGINWEDGE, Chem.BondDir.BEGINDASH):
        mol = Chem.MolFromSmiles("Fc1cccc(F)c1-c1c(Cl)cccc1Cl")
        rdDepictor.Compute2DCoords(mol)
        axis = next(
            b
            for b in mol.GetBonds()
            if not b.IsInRing() and b.GetBeginAtom().GetDegree() == b.GetEndAtom().GetDegree() == 3
        )
        side = next(
            b
            for b in axis.GetBeginAtom().GetBonds()
            if b.GetIdx() != axis.GetIdx() and b.GetBeginAtomIdx() == axis.GetBeginAtomIdx()
        )
        side.SetBondDir(direction)
        emit(f"pending/atropisomer/{direction}", Chem.MolToMolBlock(mol, forceV3000=True))
    for version in (False, True):
        mol = Chem.MolFromSmiles("CCO")
        rdDepictor.Compute2DCoords(mol)
        group = Chem.CreateMolSubstanceGroup(mol, "SUP")
        group.AddAtomWithIdx(2)
        group.SetProp("LABEL", "OH")
        emit(f"pending/substance group/{version}", Chem.MolToMolBlock(mol, forceV3000=version))


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    rng = random.Random(98153)
    syntax_cases()
    for name, original in native_molecules():
        for sample in range(3):
            order = list(range(original.GetNumAtoms()))
            rng.shuffle(order)
            mol = Chem.RenumberAtoms(original, order) if order else Chem.Mol(original)
            rdDepictor.Compute2DCoords(mol)
            for v3000 in (False, True):
                text = Chem.MolToMolBlock(mol, forceV3000=v3000)
                emit(f"{name}/{sample}/{v3000}", text)
                if sample == 0:
                    emit(f"{name}/CRLF/{v3000}", text.replace("\n", "\r\n"))
    for symbol, charge, valence, isotope in product(
        (
            "C",
            "N",
            "O",
            "S",
            "P",
            "Fe",
            "Cu",
            "H",
            "D",
            "T",
            "Cl",
            "CL",
            "R#",
            "R1",
            "*",
            "Q",
            "L",
            "LP",
            "Pol",
            "Mod",
        ),
        (0, 1, 3, 4, 5, 7),
        (0, 1, 3, 4, 15),
        (0, 1, -1),
    ):
        emit(
            f"atom/{symbol}/{charge}/{valence}/{isotope}",
            block([atom_line(symbol, charge=charge, valence=valence, mass=isotope)]),
        )
    for prop, values in (
        ("CHG", range(-4, 5)),
        ("RAD", range(5)),
        ("ISO", (-1, 0, 12, 13, 99)),
        ("HYD", (-1, 0, 1, 3)),
        ("ZCH", (-1, 0, 1)),
    ):
        for value in values:
            for prefix in ((), ("M  CHG  1   2  -1",), ("M  RAD  1   2   2",)):
                emit(
                    f"property/{prop}/{value}/{prefix}",
                    block(
                        [atom_line(charge=3), atom_line("N", charge=3)],
                        properties=(*prefix, f"M  {prop}  1   1{value:4}"),
                    ),
                )
    for order, stereo, topology in product(range(11), (0, 1, 3, 4, 6, 9), (0, 1, 2)):
        emit(
            f"bond/{order}/{stereo}/{topology}",
            block([atom_line(), atom_line("N")], [f"  1  2{order:3}{stereo:3}  0{topology:3}  0"]),
        )
    for hs in (0, 1, 2, 3):
        emit(f"hydrogen query/{hs}", block([atom_line(hs=hs)]))
    minimal = block([atom_line()])
    for i in range(len(minimal)):
        emit(f"truncated/{i}", minimal[:i])
    # Optional source-tree survey is useful locally without making CI depend on a checkout.
    source = os.environ.get("RESHIKI_RDKIT_SOURCE")
    if source:
        for path in sorted((Path(source) / "Code/GraphMol/FileParsers/test_data").glob("*.mol")):
            emit(f"source/{path.name}", path.read_text(errors="replace"))


if __name__ == "__main__":
    main()
