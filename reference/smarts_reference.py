"""Independent SMARTS parser oracle using the pinned native RDKit grammar."""

import json
import random
import re
from itertools import product
from pathlib import Path
from typing import TYPE_CHECKING

from rdkit import Chem, RDConfig, RDLogger, rdBase

if TYPE_CHECKING or __package__:
    from .cxsmarts_reference import cases as cx_cases
else:
    from cxsmarts_reference import cases as cx_cases


def cases():
    yield from cx_cases()
    for atom, extension in product(
        ("[#0]", "[#6]", "[#8]", "[H]", "[Xe]", "[!#6]", "[!!#6]"),
        (
            "",
            "|$Q$|",
            "|$R$|",
            "|$C$|",
            "|$X$|",
            "|$star_e$|",
            "|^1:0|",
            "|rb:0:2|",
            "|s:0:1|",
            "|u:0|",
        ),
    ):
        yield atom + (" " + extension if extension else "")
    atoms = [
        "C",
        "N",
        "O",
        "c",
        "*",
        "A",
        "a",
        "Cl",
        "Br",
        "[H]",
        "[13C]",
        "[nH]",
        "[#6]",
        "[!#6]",
        "[C,N]",
        "[C;H1]",
        "[C&D3]",
        "[C!H0]",
        "[R]",
        "[r5]",
        "[k6]",
        "[x2]",
        "[h]",
        "[z]",
        "[Z2]",
        "[d2]",
        "[X3]",
        "[v4]",
        "[D{2-4}]",
        "[r{-5}]",
        "[k{3-}]",
        "[^3]",
        "[+{1-3}]",
        "[-{-2}]",
        "[C@H]",
        "[C@@]",
        "[Pt@SP1]",
        "[Co@OH30]",
        "[$(C=O)]",
        "[$(C(=O)O),$(N)]",
        "[!$(c1ccccc1)]",
        "[C:12]",
    ]
    for first, bond, second in product(
        atoms,
        ("", "-", "=", "#", ":", "~", "@", "!@", "-,=", "-&!@", "->", "<-", "."),
        ("C", "[H]", "[!#6]"),
    ):
        yield first + bond + second
    for n in range(1, 119):
        symbol = Chem.GetPeriodicTable().GetElementSymbol(n)
        yield symbol
        yield f"[{symbol}]"
        yield f"[13{symbol}+2:7]"
        yield f"[#{n}]"
    for bond, suffix in product(
        (
            "@TH",
            "@AL",
            "@SP",
            "@TB",
            "@OH",
            "@'SP",
            "@",
            "!@",
            "--",
            "==",
            "\\\\\\",
            "-&=",
            "-;=",
            "!,",
            "+",
            "-><-",
        ),
        ("C", "N", "1CC1", "SP", "OH", "A", ""),
    ):
        yield "C" + bond + suffix
    for prefix, number in product(
        ("", "#", "H", "D", "d", "X", "v", "R", "r", "k", "x", "h", "z", "Z", "+", "-", "C:"),
        (
            "0",
            "00",
            "01",
            "1",
            "12",
            "127",
            "255",
            "256",
            "999",
            "65536",
            "2147483639",
            "2147483640",
            "2147483646",
            "2147483647",
            "99999999999999999999",
        ),
    ):
        yield f"[{prefix}{number}]"
    for atom, charge in product(
        ("H", "2H", "01H", "Hf", "H0", "!H", "H;", "H&"),
        ("+", "+C", "+2C", "++C", "+{1-3}", "+{1-3}C", "--C", "+:01", "+:1", "-!N"),
    ):
        yield f"[{atom}{charge}]"
    for prefix, bounds in product(
        ("D", "d", "X", "v", "R", "r", "k", "x", "h", "z", "Z", "+", "-", "H", "C", "#", "D1"),
        ("0-1", "-3", "2-", "5-2", "0-0", "-0", "0-", "1", "-", "1--2", "01-3", "1-03", ""),
    ):
        yield f"[{prefix}{{{bounds}}}]"
    for cls, n, join in product(
        ("TH", "AL", "SP", "TB", "OH", "XX"),
        range(33),
        ("", "C", "C&", "C;", "C,", "!", "C!", "C@TH1", "C@TH1;"),
    ):
        atom = f"[{join}@{cls}{n}]"
        yield atom
        yield f"[$({atom})]"
    for a, op, b in product(
        atoms, ("", "&", ";", ",", "!", "&&", ";;", "||"), ("H", "#6", "$(CC)", "@TH3")
    ):
        if a.startswith("["):
            yield f"{a[:-1]}{op}{b}]"
    for ring in (
        "0",
        "1",
        "%01",
        "%10",
        "%99",
        "%(0)",
        "%(01)",
        "%(10000)",
        "%(99999)",
        "%(100000)",
        "%1",
        "%()",
    ):
        for form in (
            "C{r}CC{r}",
            "C{r}.C{r}",
            "C{r}C{r}",
            "C{r}{r}",
            "C{r}CC",
            "C{r}CC{r}C{r}CC{r}",
            "C={r}CC#{r}",
            "C{r}(N)CC{r}",
        ):
            yield form.format(r=ring)
    for s in (
        "",
        " ",
        "\n",
        "\0",
        "C",
        "[C]",
        "C name",
        "C\nN",
        "C\0N",
        "C()",
        "C(.N)",
        "C(N.O)",
        "C((N))",
        "C(N",
        "C)N",
        "[$()]",
        "[$(C)_0]",
        "[$(C)_1]",
        "[$(C)_01]",
        "[$(C)_100]",
        "[Uut]",
        "[Uup]",
        "[C@@@]",
        "[C@ TH1]",
        "[C@'TH1]",
        "C ||",
        "C |bad|",
        "C |$hello$|",
    ):
        for prefix in ("", " ", "\t", "\n", "\x01", "酸", "🧪"):
            yield prefix + s
    # Installed reference SMARTS cover production recursive and Boolean queries.
    for path in sorted(Path(RDConfig.RDDataDir).rglob("*.txt")):
        if path.name not in ("FunctionalGroups.txt", "Crippen.txt", "FragmentDescriptors.csv"):
            continue
        for line in path.read_text().splitlines():
            if not line.startswith("#"):
                for field in line.split("\t"):
                    if "[" in field:
                        yield field.strip()
    rng = random.Random(902116)
    points = (
        "C",
        "N",
        "H",
        "#6",
        "@",
        "@TH1",
        "@TH3",
        "@SP4",
        "@OH31",
        "@TB20",
        "$(C)",
        "+",
        "-",
        "^3",
        "D2",
        "01",
    )
    for _ in range(6000):
        terms = [rng.choice(points) for _ in range(rng.randrange(2, 6))]
        expression = terms[0]
        for term in terms[1:]:
            expression += rng.choice(("", "&", ";", ",", "!", "&!", ";!", ",!")) + term
        yield f"[{expression}]"
    punctuation = "[]()012%#$@!&;,:~=-+\\/.? \n"
    for _ in range(6000):
        s = rng.choice(atoms) + rng.choice(("", "CC", "(N)O", "1CC1"))
        pos = rng.randrange(len(s) + 1)
        if rng.randrange(2):
            s = s[:pos] + rng.choice(punctuation) + s[pos:]
        else:
            s = s[:pos] + s[pos + 1 :]
        yield s


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps({"rdkit_version": rdBase.rdkitVersion}))
    for text in dict.fromkeys(cases()):
        try:
            mol = Chem.MolFromSmarts(text)
            expected = mol.GetNumAtoms() if mol is not None else None
            query = (
                re.fullmatch(
                    r"AtomAtomicNum (\d+) = val", mol.GetAtomWithIdx(0).DescribeQuery().strip()
                )
                if expected == 1
                else None
            )
            atomic_number = int(query.group(1)) if query else None
        except (RuntimeError, ValueError, OverflowError):
            expected, atomic_number = None, None
        print(json.dumps({"text": text, "expected": expected, "atomic_number": atomic_number}))


if __name__ == "__main__":
    main()
