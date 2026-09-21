"""Annotation interactions and native-written drawing fixtures for CX import."""

from itertools import product

from rdkit import Chem
from rdkit.Chem import rdDepictor


def cases():
    # Exercise optional numeric properties where native stereo does and does
    # not read them, including short-circuited unknown-stereo endpoints.
    for graph in ("F/C=C/F", "FC(Cl)=C(Br)I", "C1=CCCCC1", "F[Pt@SP1](Cl)(Br)I"):
        for index, key, value in product(
            range(6),
            ("_UnknownStereo", "_chiralPermutation", "_CanonicalRankingNumber"),
            ("bad", "2.5", "4294967295"),
        ):
            yield f"{graph} |atomProp:{index}.{key}.{value}|"
    for first, second in product(("0", "1", "bad"), repeat=2):
        yield f"F/C=C/F |atomProp:1._UnknownStereo.{first}:2._UnknownStereo.{second}|"
    graphs = (
        "[H]C",
        "[H]~C",
        "[H]C[H]",
        "[2H]C",
        "[H][C@](F)(Cl)Br",
        "F[C@H](Cl)[C@@H](Br)I",
        "[H]/N=C/F",
        "[H]C(F)=C(Cl)[H]",
        "c1ccccc1",
        "C1[C@@H](F)CC[C@H]1O",
        "*",
        "[13*]",
        "C*",
        "[C:7]",
    )
    sections = (
        "",
        "$label$",
        "$Pol_p$",
        "$Q_e$",
        "$_AV:value$",
        "SgD:0:f:d::::",
        "SgD:0,1:f:d::::",
        "SgD:0,2:f:d::::",
        "Sg:n:0",
        "Sg:n:1",
        "Sg:n:0,1",
        "Sg:n:0,1::hh:0:1",
        "SgD:0:f:d::::,SgD:0,2:f:d::::",
        "SgD:0:f:d::::,$label$",
        "a:1,3",
        "o1:1,3",
        "&1:1",
        "a:1,o1:3",
        "o1:1,o2147483649:3",
        "w:1.0",
        "wU:1.0",
        "wD:1.0",
        "c:1",
        "t:1",
        "ctu:1",
        "Z:0",
        "H:0.0",
        "Z:0,H:0.0",
        "Z:0,C:0.0",
        "H:0.0,Z:0",
        "^1:0",
        "^2:0",
        "^5:0",
        "(0,0,;1,0,;0,1,;-1,-1,;1,1,;0,-1,)",
        "(0,0,;1,0,;0,1,;-1,-1,;1,1,;0,-1,),wU:1.0",
        "(0,0,1;1,0,;0,1,;-1,-1,;1,1,;0,-1,)",
        "(0,0,1;1,0,;0,1,;-1,-1,;1,1,;0,-1,),w:1.0",
        "(nan,inf,-inf;1,0,;0,1,;-1,-1,;1,1,;0,-1,),ctu:1",
        "(0,0,;1,0,;0,1,;-1,-1,;1,1,;0,-1,),(nan,inf,-inf),wU:1.0",
    )
    for graph, section in product(graphs, sections):
        yield f"{graph} |{section}|"
    for graph, key, value in product(
        graphs,
        (
            "atomLabel",
            "dummyLabel",
            "molAtomMapNumber",
            "_chiralPermutation",
            "_UnknownStereo",
            "_NonExplicit3DChirality",
            "_CanonicalRankingNumber",
            "_CIPCode",
            "_ChiralityPossible",
        ),
        ("0", "1", "-1", "+1", "4294967295", "2.5", "bad", "true", "Q_e", "Pol_p"),
    ):
        yield f"{graph} |atomProp:0.{key}.{value}|"
    # The import parser accepts native floating-point coordinates before layout
    # replaces them. Exercise actual stereo centers at those numeric boundaries.
    for graph in ("FC(Cl)(Br)I", "CC(F)=C(Cl)Br", "c1ccccc1-c1ccccc1"):
        for scale, spatial in product((0.0, 1e-300, 1e100, 1e308), (False, True)):
            coords = []
            for index in range(12):
                x, y = ((index % 3) - 1) * scale, ((index // 3) - 1) * scale
                z = ((index % 2) * 2 - 1) * scale if spatial else 0.0
                coords.append(f"{x:.17g},{y:.17g},{z:.17g}")
            yield f"{graph} |({';'.join(coords)}),wU:1.0|"
    params = Chem.SmilesParserParams()
    params.removeHs = False
    for text in graphs:
        original = Chem.MolFromSmiles(text, params)
        if (
            original is None
            or any(a.HasQuery() for a in original.GetAtoms())
            or any(b.HasQuery() for b in original.GetBonds())
        ):
            continue
        for reverse, three_d in product((False, True), (False, True)):
            mol = Chem.Mol(original)
            if reverse:
                mol = Chem.RenumberAtoms(mol, list(reversed(range(mol.GetNumAtoms()))))
            rdDepictor.Compute2DCoords(mol)
            conf = mol.GetConformer()
            if three_d:
                conf.Set3D(True)
                for atom in mol.GetAtoms():
                    p = conf.GetAtomPosition(atom.GetIdx())
                    p.z = (atom.GetIdx() % 3 - 1) * 0.75
                    conf.SetAtomPosition(atom.GetIdx(), p)
            Chem.WedgeMolBonds(mol, conf)
            yield Chem.MolToCXSmiles(mol, canonical=False)
