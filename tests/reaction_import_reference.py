"""RXN participant state from RDKit's parser, independent of the Rust reader."""

import json
import os
import sys
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdChemReactions

if TYPE_CHECKING or __package__:
    from .molfile_groups_reference import v3 as grouped_molecule
    from .molfile_import_reference import cases as molecular_cases
    from .molfile_import_reference import native_molecules
    from .perception_reference import snapshot
    from .valence_reference import ORDERS
else:
    from molfile_groups_reference import v3 as grouped_molecule
    from molfile_import_reference import cases as molecular_cases
    from molfile_import_reference import native_molecules
    from perception_reference import snapshot
    from valence_reference import ORDERS


def read(text):
    if len(text.encode()) > 16 * 1024 * 1024:
        raise ValueError("Reaction exceeds 16 MB")
    reaction = rdChemReactions.ReactionFromRxnBlock(text, sanitize=True, removeHs=False)
    if not reaction.GetNumReactantTemplates() or not reaction.GetNumProductTemplates():
        raise ValueError("Missing reactant or product")
    rows = (reaction.GetReactants(), reaction.GetProducts(), reaction.GetAgents())
    if any(m is None for parts in rows for m in parts):
        raise ValueError("Missing reaction molecule")
    if sum(m.GetNumAtoms() for parts in rows for m in parts) > 10000:
        raise ValueError("Reaction exceeds 10,000 atoms")
    result = {}
    for role, parts in zip(("reactants", "products", "agents"), rows, strict=True):
        result[role] = []
        for original in parts:
            mol = Chem.RWMol(original)
            for atom in mol.GetAtoms():
                if atom.HasQuery():
                    if atom.DescribeQuery().strip() != f"AtomAtomicNum {atom.GetAtomicNum()} = val":
                        raise ValueError("Query reaction atom")
                    mol.ReplaceAtom(atom.GetIdx(), Chem.Atom(atom), preserveProps=True)
            Chem.SanitizeMol(mol)
            if (
                not mol.GetNumAtoms()
                or mol.GetStereoGroups()
                or any(
                    a.HasQuery() or a.GetNumRadicalElectrons() > 2 or int(a.GetChiralTag()) > 2
                    for a in mol.GetAtoms()
                )
                or any(
                    b.HasQuery() or b.GetBondType() not in ORDERS.values() or int(b.GetStereo()) > 5
                    for b in mol.GetBonds()
                )
            ):
                raise ValueError("Outside the editable reaction contract")
            conf = mol.GetConformer()
            result[role].append(
                dict(
                    molecule=dict(
                        rdkit_version=rdBase.rdkitVersion,
                        ids=list(range(1, mol.GetNumAtoms() + 1)),
                        state=snapshot(mol, "symmetric"),
                        positions=[
                            dict(x=p.x, y=p.y, z=p.z)
                            for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
                        ],
                    ),
                    annotations=dict(
                        is_3d=conf.Is3D(),
                        attachment_points=[
                            a.GetIntProp("molAttchpt") if a.HasProp("molAttchpt") else None
                            for a in mol.GetAtoms()
                        ],
                        dummy_labels=[
                            a.GetProp("dummyLabel") if a.HasProp("dummyLabel") else None
                            for a in mol.GetAtoms()
                        ],
                    ),
                )
            )
    return result


def emit(name, text):
    if os.environ.get("RESHIKI_RXN_TRACE"):
        print(name, file=sys.stderr, flush=True)
    try:
        expected, failure = read(text), None
    except (ValueError, RuntimeError, OverflowError) as error:
        expected, failure = None, str(error)
    print(json.dumps(dict(name=name, text=text, expected=expected, failure=failure)))


def v2(parts):
    return (
        "$RXN\nreference\n\n\n"
        + "".join(f"{len(row):3}" for row in parts)
        + "\n"
        + "".join("$MOL\n" + text for row in parts for text in row)
    )


def cases(emit):
    water = Chem.MolToMolBlock(Chem.MolFromSmiles("O"))
    index = 0

    def molecular(name, text):
        nonlocal index
        # Rotate the entire molecular parser corpus through all three roles.
        role = index % 3
        index += 1
        parts = [[water], [water], []]
        parts[role] = [text]
        emit(f"molecule/{role}/{name}", v2(parts))

    molecular_cases(molecular)
    for name, mol in native_molecules():
        reaction = rdChemReactions.ChemicalReaction()
        for add in (
            reaction.AddReactantTemplate,
            reaction.AddProductTemplate,
            reaction.AddAgentTemplate,
        ):
            add(mol)
        for version, writer in (
            (2000, rdChemReactions.ReactionToRxnBlock),
            (3000, rdChemReactions.ReactionToV3KRxnBlock),
        ):
            text = writer(reaction, separateAgents=True)
            emit(f"native/{name}/{version}", text)
            emit(f"native/{name}/{version}/CRLF", text.replace("\n", "\r\n"))

    minimal = v2([[water], [water], []])
    for i in range(len(minimal)):
        emit(f"truncated/v2/{i}", minimal[:i])
    for header in ("$RXN", "$RXN extra", "$RXN V2000", "$RXN V3000", "$rxn", "", " $RXN"):
        emit(f"header/{header}", header + minimal[4:])
    for counts in (
        "  1  1",
        "  1  1   ",
        "  1  1 0",
        "  1  1 -0",
        " +1 +1  0",
        "  0  1  0",
        "  1 -1  0",
        "      ",
        "１  1  1",
        "  1  1\u2000",
    ):
        lines = minimal.splitlines(keepends=True)
        lines[4] = counts + "\n"
        emit(f"counts/{counts}", "".join(lines))
    for marker in ("$MOL", "$MOL extra", "$mol", " $MOL", "$MO", ""):
        emit(f"marker/{marker}", minimal.replace("$MOL", marker))
    reaction = rdChemReactions.ChemicalReaction()
    reaction.AddReactantTemplate(Chem.MolFromSmiles("O"))
    reaction.AddProductTemplate(Chem.MolFromSmiles("CCO"))
    text = rdChemReactions.ReactionToV3KRxnBlock(reaction, separateAgents=True)
    for i in range(len(text)):
        emit(f"truncated/v3/{i}", text[:i])
    for counts in (
        "COUNTS 1 1",
        "counts\t1\t1\t0",
        "COUNTS +1 +1 0",
        "COUNTS 1 1 0 extra",
        "COUNTS 1 1 -1",
        "COUNTS 1 1 -0",
        "COUNTS 1 1 -00",
        "COUNTS -4294967295 1 0",
        "COUNTS 1 -4294967295 0",
        "COUNTS -4294967296 1 0",
        "COUNTS 4294967296 1 0",
        "COUNTS -+1 1 0",
        "COUNTS 1 1 10001",
        " COUNTS 1 1 0 ",
        "\u2000COUNTS 1 1 0\u2000",
    ):
        emit(f"v3 counts/{counts}", text.replace("COUNTS 1 1 0", counts))
    for section in ("BEGIN REACTANT", "END REACTANT", "BEGIN PRODUCT", "END PRODUCT"):
        for value in (section.lower(), section + " extra", " " + section, section[:-1], ""):
            emit(f"v3 section/{section}/{value}", text.replace(section, value))
    for suffix in ("", "garbage\n", "M  V30 BEGIN AGENT\nM  V30 END AGENT\nM  END\n"):
        emit(f"v3 suffix/{suffix}", text.removesuffix("M  END\n") + suffix)
    # Atom aliases may contain record markers; a delimiter split is not a parser.
    alias = water.replace("M  END", "A    1\n$MOL\nM  END")
    emit("alias with MOL marker", v2([[alias], [water], []]))
    alias = water.replace("M  END", "A    1\nM  END\nM  END")
    emit("alias with END marker", v2([[alias], [water], []]))
    source = os.environ.get("RESHIKI_RDKIT_SOURCE")
    if source:
        for path in sorted((Path(source) / "Code/GraphMol/ChemReactions/testData").glob("*.rxn")):
            emit(f"reaction source/{path.name}", path.read_text(errors="replace"))


def query_cases(emit):
    water = Chem.MolToMolBlock(Chem.MolFromSmiles("O"))
    for smiles in (
        "CCO",
        "[13CH3]O",
        "C[NH3+]",
        "[CH3]",
        "[2H]O",
        "c1ccccc1",
        "c1cc[nH]c1",
        "C[C@H](O)N",
        "F/C=C/F",
        "F/C=C\\F",
        "C[C@H]1CC[C@H](C)CC1",
        "[Na+]",
        "[Xe]",
        "*",
    ):
        mol = Chem.MolFromSmiles(smiles)
        for atom in mol.GetAtoms():
            number = atom.GetAtomicNum()
            for queries in (
                (f"[#{number}]",),
                (f"[!#{number}]",),
                (f"[#{number};@]",),
                ("[C,N]", f"[#{number}]"),
                (f"[#{number}]", "[C,N]"),
            ):
                records = [
                    f'{i + 1} DAT 0 ATOMS=(1 {atom.GetIdx() + 1}) QUERYTYPE=SMARTSQ QUERYOP="=" FIELDDATA="{q}"'
                    for i, q in enumerate(queries)
                ]
                text = grouped_molecule(records, atoms=smiles)
                for role in range(3):
                    parts = [[water], [water], []]
                    parts[role] = [text]
                    emit(f"query/{smiles}/{atom.GetIdx()}/{queries}/{role}", v2(parts))


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    (query_cases if "--queries" in sys.argv else cases)(emit)
