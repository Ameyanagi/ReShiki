"""Reaction SMILES participants from the original native parser and sanitizer."""

import json
import math
import random
import sys
from itertools import product
from typing import TYPE_CHECKING

from rdkit import Chem, RDLogger, rdBase
from rdkit.Chem import rdChemReactions

if TYPE_CHECKING or __package__:
    from .cxsmarts_reference import cases as extension_cases
    from .cxsmiles_cases import cases as interaction_cases
    from .smiles_prepare_reference import all_cases as molecular_cases
    from .smiles_read_reference import prepared_snapshot
    from .valence_reference import ORDERS
else:
    from cxsmarts_reference import cases as extension_cases
    from cxsmiles_cases import cases as interaction_cases
    from smiles_prepare_reference import all_cases as molecular_cases
    from smiles_read_reference import prepared_snapshot
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
                        state=prepared_snapshot(mol),
                        dummy_labels=[
                            a.GetProp("dummyLabel") if a.HasProp("dummyLabel") else None
                            for a in mol.GetAtoms()
                        ],
                    ),
                    conformers=[
                        dict(
                            positions=[
                                {
                                    key: value if math.isfinite(value) else None
                                    for key, value in zip(
                                        ("x", "y", "z"),
                                        conf.GetAtomPosition(a.GetIdx()),
                                        strict=True,
                                    )
                                }
                                for a in mol.GetAtoms()
                            ],
                            is_3d=conf.Is3D(),
                        )
                        for conf in mol.GetConformers()
                    ],
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


def cx_cases():
    for source, cases_fn in (("syntax", extension_cases), ("interactions", interaction_cases)):
        for index, text in enumerate(cases_fn()):
            body, marker, tail = text.partition("|")
            if not marker:
                continue
            for role in range(3):
                rows = ["CO", "N", "CC"]
                rows[role] = body.strip()
                yield f"cx/{source}/{index}/{role}", ">".join(rows) + " |" + tail
    # Global indices address reactants, then connected agent fragments, then
    # products. Agent ring-closure parse indices survive fragment extraction.
    for reaction in (
        "C.O>N.C1CC1.O.C2CCC2>CC(O)N",
        "(C.O)>C1CC1.N.C1.O.C1>CC",
        "C>C-2C=1CCC2C1.C1CC1>O",
        "C1CC1>C1CC1>O.C1CC1",
        "C>CC(F)Cl>F[C@](Cl)(Br)I",
        "*.[13*]>*C*>*",
        "F/C=C/F>F/C=C/F>F/C=C/F",
    ):
        for atom in range(20):
            for section in (
                f"^1:{atom}",
                f"a:{atom}",
                f"u:{atom}",
                f"atomProp:{atom}.molAtomMapNumber.123",
                "$" + ";" * atom + "Pol_p$",
                "$_AV:" + ";" * atom + "value$",
                f"Sg:n:{atom}",
                f"LN:{atom}:1.2",
                f"m:{atom}:0.1",
                "(" + ";" * atom + "bad)",
                "(" + ";" * atom + "1,2,3)",
            ):
                yield f"cx/offset/{reaction}/{section}", f"{reaction} |{section}|"
            for bond in range(20):
                for tag in ("w", "wU", "wD", "H", "C"):
                    section = f"{tag}:{atom}.{bond}"
                    yield f"cx/bond/{reaction}/{section}", f"{reaction} |{section}|"
        for bond in range(20):
            for tag in ("Z", "c", "t", "ctu"):
                yield f"cx/stereo/{reaction}/{tag}/{bond}", f"{reaction} |{tag}:{bond}|"
    rng = random.Random(551139)
    for index in range(3000):
        text = rng.choice(
            (
                "CC.O>C1CC1>CC |wU:2.1,$;;;Pol_p$|",
                "CC>N>O |(1,2,3;4,5,6;7,8,9),^1:3|",
                "[H][C@](F)(Cl)Br>CO>F/C=C/F |ctu:6,a:1|",
            )
        )
        for _ in range(rng.randrange(1, 4)):
            pos = rng.randrange(text.index("|") + 1, len(text) + 1)
            text = (
                text[:pos]
                + rng.choice(("", ",", ":", ";", "$", "|", "0", "1", "9", "酸"))
                + text[min(len(text), pos + rng.randrange(3)) :]
            )
        yield f"cx/mutation/{index}", text


def emit(name, text):
    try:
        expected, failure = read(text), None
    except (ValueError, RuntimeError, KeyError, OverflowError) as error:
        expected, failure = None, str(error)
    print(json.dumps(dict(name=name, text=text, expected=expected, failure=failure)))


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    selected = cx_cases if "--cx" in sys.argv else mutations if "--mutations" in sys.argv else cases
    for name, text in selected():
        emit(name, text)
