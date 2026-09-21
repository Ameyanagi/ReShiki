"""Reaction SMILES participants from the original native parser and sanitizer."""

import json
import random
import sys
from itertools import product
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdChemReactions

if TYPE_CHECKING or __package__:
    from .perception_reference import snapshot
    from .smiles_prepare_reference import all_cases as molecular_cases
    from .valence_reference import ORDERS
else:
    from perception_reference import snapshot
    from smiles_prepare_reference import all_cases as molecular_cases
    from valence_reference import ORDERS


def read(text):
    if len(text.encode()) > 16 * 1024 * 1024:
        raise ValueError("Reaction exceeds 16 MB")
    reaction = rdChemReactions.ReactionFromSmarts(text.strip(), useSmiles=True)
    if (
        not reaction
        or not reaction.GetNumReactantTemplates()
        or not reaction.GetNumProductTemplates()
    ):
        raise ValueError("Missing reactant or product")
    rows = (reaction.GetReactants(), reaction.GetProducts(), reaction.GetAgents())
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
            result[role].append(
                dict(
                    prepared=dict(
                        state=snapshot(mol, "symmetric"),
                        dummy_labels=[
                            a.GetProp("dummyLabel") if a.HasProp("dummyLabel") else None
                            for a in mol.GetAtoms()
                        ],
                    ),
                    conformers=[],
                    name=None,
                )
            )
    return result


def cases():
    for name, text in molecular_cases():
        if "|" in text:
            continue
        # Rotate every native molecular fixture through all three roles. These
        # imports retain explicit hydrogen atoms and do not run legacy CIP.
        for role in range(3):
            rows = ["O", "", "O"]
            rows[role] = text
            yield f"molecule/{name}/{role}", ">".join(rows)
    for item in (
        "C",
        "C.O",
        "(C.O)",
        "((C.O))",
        "(C).O",
        "(C.O).N",
        "C1.C1",
        "(C1.C1)",
        "[H][C@](F)(Cl)Br",
        "C/C=C/C",
        "[Na+].[Cl-]",
        "N->[Cu]",
        "[Cu]<-N",
        "C.",
        ".C",
        "C..O",
        "",
        ".",
        "()",
        "(C)N",
        "(C))",
        "((C)",
        "C((O))",
        "C)O(",
        "C(O).(N.C)",
        "C1.C1.O",
        "C1.O.N1",
    ):
        for role in range(3):
            rows = ["C", "O", "N"]
            rows[role] = item
            yield f"group/{role}/{item}", ">".join(rows)
    for before, after, sep in product(("", " ", "\t", "\n", "\r", "\v", "\f", "\u3000"), repeat=3):
        text = f"{before}C{sep}>{sep}O{sep}>{sep}N{after}"
        yield f"spaces/{before!r}/{after!r}/{sep!r}", text
        yield (
            f"dots/{before!r}/{after!r}/{sep!r}",
            f"{before}(C{sep}.{sep}O)>N{sep}.{sep}C>(O.C){after}",
        )
    for text in (
        "C>>O named",
        "C>>O 名前",
        "C>>O\tname",
        "C>>O name >",
        "C>>O>name",
        "C>name",
        ">>O",
        "C>>",
        ">C>",
        "C>>>O",
        "C>O>N>C",
        "C- >O>>N",
        "C ->O>>N",
        "N->[Cu]>>N",
        "N>[Cu]<-N>O",
        "C\n>>O",
        "C>>O\nname",
        "C>>O\x00ignored",
        "C>>O\x00>C",
        "C\x00>>O",
        "\x1cC>>O\x1f",
        "\0C>>O",
        "C>>O酸",
        "C>>酸O",
        "酸C>>O",
    ):
        yield f"framing/{text!r}", text
    for count in (1, 2, 100, 9998, 9999, 10000):
        yield f"limit/atoms/{count}", "C" * count + ">>O"
        yield f"limit/agents/{count}", "C>" + ".".join(["O"] * count) + ">C"


def mutations():
    rng = random.Random(270993)
    for index in range(3000):
        text = rng.choice(
            (
                "CCO.O>O>CC=O",
                "(C.O)>N->[Cu]>C1CC1",
                "[H][C@](F)(Cl)Br>>C",
                "C>C1.C1.O>O",
                "(C1.C1)>>N",
            )
        )
        for _ in range(rng.randrange(1, 4)):
            position = rng.randrange(len(text) + 1)
            end = min(len(text), position + rng.randrange(3))
            text = (
                text[:position]
                + rng.choice(("", ".", "(", ")", ">", "->", "\t", "\n", "\r", "\x00", " ", "酸"))
                + text[end:]
            )
        yield f"mutation/{index}/{text!r}", text


def emit(name, text):
    try:
        expected, failure = read(text), None
    except (ValueError, RuntimeError, KeyError, OverflowError) as error:
        expected, failure = None, str(error)
    print(json.dumps(dict(name=name, text=text, expected=expected, failure=failure)))


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    for name, text in mutations() if "--mutations" in sys.argv else cases():
        emit(name, text)
