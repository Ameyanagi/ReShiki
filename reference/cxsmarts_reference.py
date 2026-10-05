"""CXSMARTS corpus; every expected result comes from native MolFromSmarts."""

import random
from itertools import product


def cases():
    for whitespace in ("\t", "\n", "\v", "\f", "\r", " "):
        yield f"C {whitespace}|u:0|{whitespace}名前{whitespace}"
        yield f"C {whitespace}|^1:0|{whitespace}名前{whitespace}"
    graphs = (
        "C",
        "CC",
        "CCC",
        "CC(O)Cl",
        "C1CCCCC1",
        "C2C1CCC2C1",
        "C1.C1",
        "[$(CCC)]CC",
        "C[$(C1CC1)]C",
        "N->[Cu]<-N",
        "C/C=C/C",
        "C.C.C",
        "C-1CC1",
        "C-2C=1CCC2C1",
        "C1CC11CC1",
        "[$(CCCCCC)]C1CC1",
        "C1[$(CCC)]CC1",
    )
    sections = (
        "",
        " ",
        "unknown",
        "foo",
        "bad",
        "1,2",
        "&",
        "&#44;",
        "^1:0",
        "^0:0",
        "^8:0",
        "^1:",
        "$hello$",
        "$_AV:hello$",
        "$酸;英語$",
        "$&#59;&#36;&#124;$",
        "$&#;$",
        "$&#4294967295;$",
        "$unterminated",
        "atomProp:0.name.value",
        "atomProp:0..value",
        "atomProp:0.name.",
        "atomProp:0.name.酸",
        "atomProp:99.x.y",
        "atomProp:bad",
        "atomProp:0.atomLabel.Q_e",
        "atomProp:0._chiralPermutation.bad",
        "()",
        "(0,0,)",
        "(1,2,3;4,5,6)",
        "(invalid)",
        "(0;invalid)",
        "(nan,inf,-inf)",
        "(1,2,3,invalid)",
        "a:",
        "a:0",
        "a:0,0",
        "a:99,99",
        "o:0",
        "o1:0",
        "&1:0",
        "a:0,a:0",
        "o1:0,o1:0",
        "u:",
        "u:0",
        "u:99",
        "u",
        "rb:0:0",
        "rb:0:1",
        "rb:0:2",
        "rb:0:4",
        "rb:0:*",
        "s:0:*",
        "s:0:100",
        "LN:1:1.3",
        "LN:0:1.3",
        "LN:99:1.3",
        "LN:0:1.3.1.2",
        "LN:0:1.3.1x2",
        "m:0:1.2",
        "m:1:0.2",
        "m:99:0.1",
        "m:0:",
        "m:0:1..2",
        "m:0:1.2,1:0",
        "C:0.0",
        "H:0.0",
        "C:99.0",
        "C:0.99",
        "Z:0",
        "w:0.0",
        "wU:0.0",
        "wD:1.0",
        "wU:0.0,wD:1.0",
        "wX:0.0",
        "wU:",
        "c:0",
        "t:0",
        "ctu:0",
        "ctuxxx:0",
        "c:99",
        "c",
        "SgD:0:name:data::::",
        "SgD:99:name:data::::",
        "SgD:0:|",
        "SgD:0:name:data::::(1,2)",
        "SgD:0:name:data",
        "SgD:0:name:data::::,SgH:0:0",
        "SgD:0:name:data::::,SgH:0:1",
        "Sg:n:0",
        "Sg:n:1::hh",
        "Sg:n:99",
        "Sg:unknown:0",
        "Sg:n:0::hh:0:1",
        "Sg:n:0::hh:99:99",
        "Sg:n:0,SgH:0:1",
        "Sg:n:99,Sg:n:0,SgH:1:1",
        "SgH:99:99",
        "SgH:0:",
    )
    for graph, section in product(graphs, sections):
        yield f"{graph} |{section}|"
    for graph, kind, atom, bond in product(
        graphs,
        ("C", "H", "w", "wU", "wD"),
        range(8),
        range(8),
    ):
        yield f"{graph} |{kind}:{atom}.{bond}|"
    for kind, value in product(
        (
            "C:",
            "Z:",
            "u:",
            "s:0:",
            "rb:0:",
            "^1:",
            "o",
            "a:",
            "LN:1:1.",
            "Sg:n:",
            "atomProp:",
            "wU:",
        ),
        (
            "",
            "0",
            "00",
            "01",
            "99",
            "4294967295",
            "4294967296",
            "999999999999999999999999",
            "-1",
            "+1",
            ",0",
            ",4294967296",
            "0,,4294967296",
        ),
    ):
        yield f"CCC |{kind}{value}|"
    for coord in (
        "NaN",
        "+nan",
        "-nan",
        "nan(x)",
        "NAN(1)",
        "inf",
        "+INF",
        "Infinity",
        "1e309",
        "1e-999",
        " 1",
        "1 ",
        "1f",
        "0x1p0",
        "&#49;",
        "&#255;",
        "&#2147483647;",
        "&#2147483648;",
        "&#-1;",
        "&#x;",
        "&#;",
    ):
        yield f"C |({coord})|"
        yield f"C |(0;{coord})|"
    for kind, atoms, bonds in product(
        (
            "n",
            "mon",
            "mer",
            "co",
            "xl",
            "mod",
            "mix",
            "f",
            "any",
            "gen",
            "c",
            "grf",
            "alt",
            "ran",
            "blk",
        ),
        ("0", "1", "0,1", "0,0", "99", "99,0", ""),
        ("", ":0", ":0:1", ":1:2", ":2:3", ":99:99", ":0,0:1,1"),
    ):
        yield f"CCC |Sg:{kind}:{atoms}::hh{bonds}|"
    rng = random.Random(100921)
    for mantissa, exponent in product(
        (
            "0",
            "1",
            "1.1",
            "1.8",
            "1.fffffffffffff",
            "1.fffffffffffff7",
            "1.fffffffffffff8",
            "0.fffffffffffff8",
            "0.0000000000001",
            "123456789abcdef1234567",
            "1.123456789abcdef",
        ),
        (-2000, -1076, -1075, -1074, -1024, -1023, -1022, 0, 1022, 1023, 1024, 2000),
    ):
        yield f"C |(0x{mantissa}p{exponent})|"
    for number in (
        "1e-307",
        "1e-308",
        "2.2250738585072014e-308",
        "2.2250738585072013e-308",
        "5e-324",
        "0e999",
        "0e-999",
        "0x1",
        "0x1.1",
        "0x.1p2",
        "0x1p",
        "-0x1p1",
        "0x0p9999999999999",
    ):
        yield f"C |({number})|"
    for _ in range(2000):
        graph = rng.choice(graphs)
        section = ",".join(rng.choices(sections, k=rng.randrange(2, 5)))
        yield f"{graph} |{section}|"
    for group in ("a", "o0", "o1", "&0", "&1", "o2147483649", "&4294967295"):
        for other in ("a", "o0", "o1", "&0", "&1", "o2147483649", "&4294967295"):
            yield f"CC |{group}:0,{other}:0|"
    for _ in range(2500):
        graph = rng.choice(graphs)
        section = rng.choice(sections)
        index = rng.randrange(len(section) + 1)
        if rng.randrange(2):
            section = section[:index] + rng.choice("0:;.,|$&^()-+ abcXYZ") + section[index:]
        else:
            section = section[:index] + section[index + 1 :]
        yield f"{graph} |{section}|"
    for section in sections:
        yield f"C |{section}"
        yield f"C |{section}| named 酸"
        for end in range(len(section)):
            yield f"CCC |{section[:end]}|"
