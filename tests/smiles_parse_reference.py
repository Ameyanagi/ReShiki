"""Independent unsanitized SMILES graph oracle, using the pinned native reader."""

import json
import os
import random
from itertools import product
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .kekulize_reference import directions
    from .ranking_reference import metadata
    from .valence_reference import graph
else:
    from kekulize_reference import directions
    from ranking_reference import metadata
    from valence_reference import graph


def read(text):
    params = Chem.SmilesParserParams()
    params.sanitize = False
    params.removeHs = False
    params.allowCXSMILES = False
    params.parseName = False
    mol = Chem.MolFromSmiles(text, params)
    if mol is None:
        raise ValueError("Native reader rejected SMILES")
    if any(a.HasQuery() for a in mol.GetAtoms()) or any(b.HasQuery() for b in mol.GetBonds()):
        raise ValueError("Outside the editable molecular graph contract")
    return mol


def cases():
    if source := os.environ.get("RESHIKI_RDKIT_SOURCE"):
        directory = Path(source) / "Code/GraphMol/SmilesParse/test_data"
        for path in sorted([*directory.glob("*.smi"), *directory.glob("*.cxsmi")]):
            for i, line in enumerate(path.read_text().splitlines()):
                # This suite checks the bare graph; CX semantics are a later
                # import stage. Strip the source fixture's extension/name only.
                fields = line.split()
                if fields:
                    yield f"source/{path.name}/{i}", fields[0]
    for text in (
        "C1CC1CC",
        "C12CC2CC1CC",
        "C21CC1CC2CC",
        "C1CC11CC1CC",
        "C1.C1CC",
        "C2C1C3CC1CC2C3",
        "N->1CC[Cu]1CC",
    ):
        yield f"index/{text}", text
    for cls, maximum in (("SP", 3), ("TB", 20), ("OH", 30), ("AL", 2), ("TH", 2)):
        for permutation, degree, hs in product(range(maximum + 2), range(8), ("", "H")):
            tag = cls + (str(permutation) if permutation else "")
            center = f"[Pt@{tag}{hs}]"
            ligands = ("F", "Cl", "Br", "I", "N", "O", "C")[:degree]
            branches = "".join(f"({s})" for s in ligands)
            yield f"coordination/start/{tag}/{degree}/{hs}", center + branches
            yield f"coordination/incoming/{tag}/{degree}/{hs}", "C" + center + branches
            if degree:
                # Ring closures are appended in numeric-label order, which is
                # different from text order in half of these cases.
                labels = list(range(degree))
                if permutation % 2:
                    labels.reverse()
                rings = "".join(map(str, labels))
                fragments = "".join(
                    f".{atom}{label}" for atom, label in zip(ligands, labels, strict=True)
                )
                yield f"coordination/rings/{tag}/{degree}/{hs}", center + rings + fragments
                yield (
                    f"coordination/incoming-rings/{tag}/{degree}/{hs}",
                    "C" + center + rings + fragments,
                )
    atoms = (
        "C",
        "N",
        "O",
        "c",
        "*",
        "Cl",
        "Br",
        "[H]",
        "[2H]",
        "[13CH3]",
        "[NH4+]",
        "[nH]",
        "[#0]",
        "[#6]",
        "[C@H]",
        "[C@@]",
        "[C:0]",
        "[C:7]",
        "[Fe+2]",
    )
    for a, bond, b in product(
        atoms,
        ("", "-", "=", "#", ":", "$", "/", "\\", "\\\\", "->", "<-", "~", "."),
        ("C", "c", "[H]"),
    ):
        yield f"atoms/{a}{bond}{b}", a + bond + b
    for n in range(1, 119):
        symbol = Chem.GetPeriodicTable().GetElementSymbol(n)
        for text in (symbol, f"[{symbol}]", f"[13{symbol}H2+2:0]", f"[#{n}]", f"['{symbol}']"):
            yield f"element/{text}", text
    for number, shape in product(
        (
            "0",
            "00",
            "01",
            "1",
            "12",
            "127",
            "128",
            "255",
            "256",
            "65535",
            "65536",
            "2147483639",
            "2147483640",
            "2147483647",
            "999999999999999999999",
        ),
        ("[{}C]", "[C+{}]", "[C-{}]", "[CH{}]", "[C:{}]", "[#{}]", "[HH{}]"),
    ):
        text = shape.format(number)
        yield f"number/{text}", text
    for symbol, chiral, hs, charge in product(
        ("C", "H", "n", "S", "13C"),
        ("", "@", "@@", "@TH", "@TH1", "@TH2", "@TH0", "@TH3", "@'TH2", "@SP1", "@AL2", "@OH30"),
        ("", "H", "H0", "H1", "H2"),
        ("", "+", "++", "+++", "--", "+0", "-2"),
    ):
        atom = f"[{symbol}{chiral}{hs}{charge}]"
        yield f"brackets/{atom}", f"F{atom}(Cl)(Br)I"
    for label, first, last in product(
        ("0", "1", "9", "%10", "%99", "%(0)", "%(001)", "%(12345)", "%01", "%(123456)", "%(1"),
        ("", "-", "=", ":", "/", "\\", "->", "<-", "~"),
        ("", "-", "=", ":", "/", "\\", "->", "<-", "~"),
    ):
        text = f"C{first}{label}CC{last}{label}"
        yield f"ring/{text}", text
    for text in (
        "",
        " ",
        "  C  ",
        "\tC\t",
        "C name",
        "C\nN",
        "C\rN",
        "C\x00N",
        "C酸",
        "酸C",
        "C |$a$|",
        "C(",
        "C()",
        "(C)",
        "C(C))",
        "C((C))",
        "C(.N)",
        "C(C.N)",
        "C(C.N)O",
        "C.(O)",
        "C..N",
        ".C",
        "C.",
        "C1C1",
        "C11",
        "C1CC1",
        "C1CC11CC1",
        "C12CC1CC2",
        "C21CC1CC2",
        "C1.C1",
        "C12.C12",
        "C1CC",
        "[C@@](F)1(C)CCO1",
        "C1CN[C@](O)(N)1",
        "[C@](Cl)(F)1CC[C@H](F)CC1",
        "[C@@]1(Cl)(F)I.Br1",
        "[C@@](Cl)1(F)I.Br1",
        "[C@@](Cl)(F)1I.Br1",
        "[C@@](Cl)(F)(I)1.Br1",
        "C[S@]2(=O).Cl2",
        "F[C@]1CCO1",
        "[C@H](F)(Cl)Br",
        "N.[C@H](F)(Cl)Br",
        "c1ccccc1",
        "c~1ccccc1",
        "c1ccccc~1",
        "c~1ccccc-1",
        "F/C=C/F",
        "F/C=C\\F",
        "C/C(Cl)=C(Br)/F",
        "[nH]1cccc1",
        "C1CC[C@@H]2CCCC[C@H]12",
        "[Uut]",
        "[Uuo]",
        "[R]",
        "[D]",
        "[T]",
        "[si]",
        "[as]",
        "[se]",
        "[te]",
        "A",
        "a",
        "[C@@@]",
    ):
        yield f"edge/{text!r}", text
    # Installed RDKit data provides varied real structures on every CI platform.
    path = Path(RDConfig.RDDataDir) / "NCI/first_5K.smi"
    for i, line in enumerate(path.read_text().splitlines()):
        fields = line.split()
        if fields:
            yield f"NCI/{i}", fields[0]
    templates = json.loads(
        (Path(__file__).resolve().parents[1] / "assets/templates.json").read_text()
    )
    for template in templates:
        if template.get("smiles"):
            yield f"template/{template['smiles']}", template["smiles"]
    rng = random.Random(824773)
    seeds = [
        "CC(C)(O)CC",
        "c1ccc2ccccc2c1",
        "F[C@]1(Cl)CCC1",
        "N->[Cu]<-N",
        "F/C=C/F",
        "[13CH3:1][NH3+:2]",
    ]
    tokens = list("CNcOn[]()012%=#-+/\\.:@") + ["Br", "Cl", "[H]", "->", "酸", "\x00"]
    for i in range(3500):
        text = rng.choice(seeds)
        for _ in range(rng.randrange(1, 4)):
            pos = rng.randrange(len(text) + 1)
            text = text[:pos] + rng.choice(tokens) + text[pos + rng.randrange(2) :]
        yield f"mutation/{i}", text


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    for name, text in cases():
        try:
            mol = read(text)
            expected = dict(
                graph=graph(mol),
                metadata=metadata(mol),
                directions=directions(mol),
                dummy_labels=[
                    a.GetProp("dummyLabel") if a.HasProp("dummyLabel") else None
                    for a in mol.GetAtoms()
                ],
            )
            if name.startswith("index/"):
                # Query the public CX reader for its bond index mapping. This
                # avoids reproducing ring-closure bookkeeping in the oracle.
                params = Chem.SmilesParserParams()
                params.sanitize = False
                params.removeHs = False
                indices = [None] * mol.GetNumBonds()
                for index in range(mol.GetNumBonds()):
                    marked = Chem.MolFromSmiles(f"{text} |Z:{index}|", params)
                    if marked is None:
                        raise RuntimeError("Reference CX bond marker failed")
                    zeros = [
                        b.GetIdx()
                        for b in marked.GetBonds()
                        if b.GetBondType() == Chem.BondType.ZERO
                    ]
                    if len(zeros) != 1:
                        raise RuntimeError("Reference CX bond marker is ambiguous")
                    indices[zeros[0]] = index
                expected["bond_indices"] = indices
            failure = None
        except (RuntimeError, ValueError, OverflowError) as error:
            expected, failure = None, str(error)
        print(json.dumps(dict(name=name, text=text, expected=expected, failure=failure)))


if __name__ == "__main__":
    main()
