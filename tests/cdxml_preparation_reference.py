"""Capture the unchanged original import prefix, before scene assembly.

The AST retains every original statement through AssignStereochemistry. Inserted
assignments only label failure stages and copy parts at the bond-reader boundary;
all chemistry and XML transformations still execute the original helpers.
"""

import ast
import copy
import hashlib
import inspect
import itertools
import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from cdxml_molecular_reference import from_mol, xml
from kekulize_reference import directions
from perception_reference import snapshot
from ranking_reference import metadata
from rdkit import Chem, RDConfig, RDLogger, rdBase
from valence_reference import graph

from engine import worker


def prefix():
    source = inspect.getsource(worker.import_cdxml)
    tree = ast.parse(source)
    function = tree.body[0]
    assert isinstance(function, ast.FunctionDef)
    body = []
    stage = "validation"
    for statement in function.body:
        if isinstance(statement, ast.Assign):
            target = statement.targets[0]
            if isinstance(target, ast.Name):
                name = target.id
                if name == "read_text":
                    break
                stage = {
                    "abbreviated": "expansion",
                    "parts": "parser",
                    "object_map": "bindings",
                    "mol": "combination",
                    "document_style": "style",
                    "scale": "scaling",
                    "base": "bonds",
                }.get(name, stage)
        if isinstance(statement, ast.For) and isinstance(statement.target, ast.Name):
            if statement.target.id == "bond":
                stage = "restoration"
        if isinstance(statement, ast.Expr) and isinstance(statement.value, ast.Call):
            call = statement.value.func
            if isinstance(call, ast.Attribute):
                stage = {
                    "SanitizeMol": "sanitization",
                    "AssignChiralTypesFromBondDirs": "chirality",
                    "DetectBondStereochemistry": "detection",
                    "AssignStereochemistry": "legacy",
                }.get(call.attr, stage)
        body.extend(ast.parse(f"_preparation_stage = {stage!r}").body)
        if stage == "bonds":
            body.extend(ast.parse("_source_parts = [Chem.Mol(part) for part in parts]").body)
        body.append(statement)
    assert stage == "legacy"
    body.extend(ast.parse("return locals()").body)
    function.body = body
    ast.fix_missing_locations(tree)
    namespace = vars(worker).copy()
    exec(compile(tree, "<original import_cdxml preparation>", "exec"), namespace)
    return namespace["import_cdxml"], hashlib.sha256(source.encode()).hexdigest()


PREPARE, PREFIX_HASH = prefix()


def points(mol):
    if not mol.GetNumConformers():
        return []
    conf = mol.GetConformer()
    return [
        dict(x=p.x, y=p.y, z=p.z)
        for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
    ]


def part_state(mol):
    return dict(
        id=mol.GetIntProp("CDX_FRAG_ID"),
        graph=graph(mol),
        metadata=metadata(mol),
        directions=directions(mol),
        positions=points(mol),
        is_3d=mol.GetConformer().Is3D() if mol.GetNumConformers() else False,
        atom_cip_ranks=[
            a.GetUnsignedProp("_CIPRank") if a.HasProp("_CIPRank") else None for a in mol.GetAtoms()
        ],
    )


def tree_state(root):
    return dict(
        tag=root.tag,
        attributes=dict(root.attrib),
        text=root.text or "",
        tail=root.tail or "",
        children=[tree_state(child) for child in root],
    )


def capture(scope):
    mol, root = scope["mol"], scope["root"]
    ordinals = {node: i for i, node in enumerate(root.iter())}
    return dict(
        molecule=dict(
            rdkit_version=rdBase.rdkitVersion,
            ids=list(range(1, mol.GetNumAtoms() + 1)),
            positions=points(mol),
            state=snapshot(mol, "symmetric"),
        ),
        expanded=tree_state(root),
        abbreviations=scope["abbreviated"],
        fragments=[part_state(part) for part in scope["_source_parts"]],
        fragment_bindings=[
            dict(source=ordinals[node], atoms=ids) for node, ids in scope["object_map"].items()
        ],
        drawing_style=scope["document_style"],
        bonds=[
            dict(bond, z_order=str(bond["z_order"]), color=[float(c) for c in bond["color"]])
            for bond in scope["base"]["bonds"]
        ],
        palette=dict(colors=[[float(c) for c in color] for color in worker.palette(root)]),
        source_scale=scope["scale"],
        conformer_scale=scope["factor"],
        conformer_3d=mol.GetConformer().Is3D() if mol.GetNumConformers() else None,
    )


def emit(name, text, restriction=None, application=False):
    if not isinstance(text, str):
        text = ET.tostring(text, encoding="unicode")
    expected = failure = failure_stage = None
    try:
        expected = capture(PREPARE(text))
    except (ValueError, RuntimeError, KeyError, IndexError, OverflowError, ET.ParseError) as error:
        failure = str(error)
        tb = error.__traceback__
        while tb:
            if tb.tb_frame.f_code.co_name == "chemistry_xml":
                failure_stage = "normalization"
            if tb.tb_frame.f_code.co_filename == "<original import_cdxml preparation>":
                failure_stage = tb.tb_frame.f_locals.get("_preparation_stage")
            tb = tb.tb_next
        if failure_stage is None:
            raise
    application = application or restriction is not None
    application_document = application_failure = None
    if application:
        try:
            application_document = worker.handle(
                dict(protocol=1, operation="import", format="cdxml", text=text)
            )["document"]
        except (
            ValueError,
            RuntimeError,
            KeyError,
            IndexError,
            OverflowError,
            ET.ParseError,
        ) as error:
            application_failure = str(error)
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                expected=expected,
                failure=failure,
                failure_stage=failure_stage,
                restriction=restriction,
                application_checked=application,
                application_document=application_document,
                application_failure=application_failure,
            )
        )
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion, prefix_sha256=PREFIX_HASH)))
    for body in ("<page/>", '<page><fragment id="1"/></page>'):
        emit("empty/" + body, "<CDXML>" + body + "</CDXML>")
    for length in (None, "5", "14.4", "30", "100", "4.99", "100.01", "nan", "inf", "bad"):
        single = ET.fromstring(xml([{}, {"Element": "8"}], [(0, 1, {})], length=length))
        emit("single-scale/" + str(length), single)
        page = single.find("page")
        assert page is not None
        extra = copy.deepcopy(page[0])
        extra.set("id", "7001")
        for element in extra.iter():
            if element.get("id") and element.tag != "fragment":
                element.set("id", str(int(element.get("id")) + 10000))
            for key in ("B", "E"):
                if element.get(key):
                    element.set(key, str(int(element.get(key)) + 10000))
        page.append(extra)
        emit("multi-scale/" + str(length), single)
    for empty_first in (False, True):
        root = ET.fromstring(xml([{}]))
        page = root.find("page")
        assert page is not None
        page.insert(0 if empty_first else 1, ET.Element("fragment", id="8000"))
        emit(f"empty-combine/{empty_first}", root)
    for element, charge, hydrogens in itertools.product(range(119), (-2, 0, 2), (None, "0", "2")):
        attrs = dict(Element=str(element), Charge=str(charge))
        if hydrogens is not None:
            attrs["NumHydrogens"] = hydrogens
        emit(f"atom/{element}/{charge}/{hydrogens}", xml([attrs]))
    for order, display, secondary in itertools.product(
        ("1", "2", "3", "4", "1.5", "hydrogen", "dative"),
        tuple(worker.DISPLAY)
        if hasattr(worker, "DISPLAY")
        else (
            "Solid",
            "Dash",
            "Dot",
            "Bold",
            "Hash",
            "WedgeBegin",
            "WedgeEnd",
            "WedgedHashBegin",
            "WedgedHashEnd",
            "HollowWedgeBegin",
            "HollowWedgeEnd",
            "Wavy",
        ),
        (None, "Solid", "Dash", "Bold", "DottedHydrogen"),
    ):
        attrs = dict(Order=order, Display=display)
        if secondary:
            attrs["Display2"] = secondary
        emit(f"bond/{order}/{display}/{secondary}", xml([{}, {}], [(0, 1, attrs)]))
    for smiles in (
        "c1ccccc1",
        "c1ccncc1",
        "c1cc[nH]c1",
        "C[C@H](O)F",
        "C[C@@H](O)F",
        "F/C=C/F",
        "F/C=C\\F",
        "[2H][C@](F)(Cl)Br",
        "O=N(=O)c1ccccc1",
        "[O-][N+](=O)c1ccccc1",
        "C1=CC2=CC=CC=C2C=C1",
        "[CH2]C",
        "[O]",
        "[NH4+]",
        "N->[Cu+2]<-N",
        "C=C=C",
        "C1=C=CCCCC1",
    ):
        mol = Chem.MolFromSmiles(smiles)
        if mol is None:
            raise ValueError(smiles)
        for variant in range(4):
            emit(f"molecule/{smiles}/{variant}", from_mol(mol, variant=variant))
    data = Path(RDConfig.RDDataDir) / "NCI" / "first_5K.smi"
    for i, line in enumerate(data.read_text().splitlines()[:250]):
        mol = Chem.MolFromSmiles(line.split()[0])
        if mol is not None:
            emit(f"nci/{i}", from_mol(mol, variant=i % 4))
    for fixture in sorted(Path(__file__).with_name("fixtures").glob("*.cdxml")):
        emit("fixture/" + fixture.name, fixture.read_text())
    for predicate in (
        "RingBondCount",
        "UnsaturatedBonds",
        "SubstituentsUpTo",
        "SubstituentsExactly",
        "FreeSites",
        "LinkCountLow",
        "LinkCountHigh",
        "IsotopicAbundance",
        "Topology",
        "RxnChange",
        "RxnStereo",
        "RxnParticipation",
    ):
        for value in ("", "0", "-1", "1", "Unspecified", "no", "yes"):
            emit(f"predicate/{predicate}/{value}", xml([{predicate: value}]))
    for source in (
        "<CDXML/>",
        "<CDXML><page/><page/></CDXML>",
        "<CDXML><page><unsupported/></page></CDXML>",
        '<CDXML><page><embeddedobject><n Topology="Ring"/></embeddedobject></page></CDXML>',
        "<CDXML><page><embeddedobject><unsupported/></embeddedobject></page></CDXML>",
        '<CDXML xmlns="urn:x"><page/></CDXML>',
    ):
        emit("validation/" + source, source)
    for extra, restriction in (
        ('xmlns:q="urn:unused"', None),
        ('q:a="1" xmlns:q="urn:test"', "namespaced attributes"),
    ):
        emit("namespace/" + extra, f"<CDXML {extra}><page/></CDXML>", restriction)
    emit(
        "dtd/internal",
        '<!DOCTYPE CDXML [<!ENTITY a "x">]><CDXML><page/></CDXML>',
        "internal DTD declaration",
    )
    for source in ("<page/>", "<group><page/></group>"):
        emit("non-CDXML-root/" + source, source, application=True)
    from cdxml_abbreviations_reference import drawing

    for attached, connection in itertools.product((False, True), repeat=2):
        emit(
            f"abbreviation/{attached}/{connection}",
            drawing(attached=attached, connection=connection),
        )
    for source in (
        '<page><t p="0 0"><s>x</s></t></page>',
        '<group><page><t p="0 0"><s>x</s></t></page></group>',
        '<group><CDXML><page><fragment id="1"><n id="2" p="0 0"/></fragment></page></CDXML></group>',
    ):
        emit("non-CDXML-root/nonempty/" + source, source, application=True)
    for source, restriction in (
        (
            '<CDXML q:a="1" xmlns:q="urn:test"><page><t p="0 0"><s>x</s></t></page></CDXML>',
            "namespaced attributes",
        ),
        (
            '<!DOCTYPE CDXML [<!ENTITY a "x">]><CDXML><page><t p="0 0"><s>&a;</s></t></page></CDXML>',
            "internal DTD declaration",
        ),
    ):
        emit("nonempty-restriction/" + restriction, source, restriction)
    emit(
        "raw-hydrogen-application",
        xml([{}, {}], [(0, 1, dict(Order="hydrogen"))]),
        application=True,
    )
    emit(
        "display-hydrogen-application",
        xml([{}, {}], [(0, 1, dict(Order="1", Display="Dash", Display2="DottedHydrogen"))]),
        application=True,
    )
    # Winding, coordinate geometry and explicit E/Z fallback survive restoration.
    for display, mirror, rotation, explicit_h in itertools.product(
        (
            "WedgeBegin",
            "WedgeEnd",
            "WedgedHashBegin",
            "WedgedHashEnd",
            "HollowWedgeBegin",
            "Hash",
            "Bold",
            "Wavy",
        ),
        (1, -1),
        range(4),
        (None, "1"),
    ):
        xy = [(0, 0), (14.4, 0), (-7.2, 12.47), (-7.2, -12.47)]
        for _ in range(rotation):
            xy = [(-y, x) for x, y in xy]
        atoms = [
            dict(p=f"{x * mirror} {y}", Element=str(n))
            for (x, y), n in zip(xy, (6, 9, 17, 35), strict=True)
        ]
        if explicit_h:
            atoms[0]["NumHydrogens"] = explicit_h
        emit(
            f"winding/{display}/{mirror}/{rotation}/{explicit_h}",
            xml(atoms, [(0, 1, dict(Display=display)), (0, 2, {}), (0, 3, {})]),
        )
    for display, bs, shape in itertools.product(("Solid", "Wavy"), (None, "E", "Z"), range(3)):
        atoms = [
            dict(p=p, Element=str(n))
            for p, n in zip(
                ("0 0", "14.4 0", "-7.2 12.47", f"21.6 {12.47 if shape else -12.47}"),
                (6, 6, 9, 17),
                strict=True,
            )
        ]
        attrs = dict(Order="2", Display=display)
        if bs:
            attrs["BS"] = bs
        emit(f"alkene/{display}/{bs}/{shape}", xml(atoms, [(0, 1, attrs), (0, 2, {}), (1, 3, {})]))
    for display in ("", "bad", "Solid"):
        for secondary in ("", "bad", "Dot"):
            emit(
                f"invalid-display/{display}/{secondary}",
                xml([{}, {}], [(0, 1, dict(Display=display, Display2=secondary))]),
            )
    for fixture in sorted(
        Path(__file__).with_name("fixtures").joinpath("cdxml-molecular").glob("*.cdxml")
    ):
        emit("molecular-fixture/" + fixture.name, fixture.read_text())
    for z in ("0", "1", "-1", "0.0000152587890625"):
        mol = Chem.MolFromSmiles("C[C@H](F)Cl")
        root = ET.fromstring(from_mol(mol))
        for i, node in enumerate(root.iter("n")):
            node.set("xyz", node.get("p") + " " + (z if i % 2 else "0"))
        emit("3D/" + z, root)
    for length, x in itertools.product(
        ("5", "14.4", "17.271823", "30", "100"),
        ("0", "-0", "0.0000152587890625", "123.456789", "-12.125"),
    ):
        emit(f"exact-scale/{length}/{x}", xml([dict(p=x + " -0")], length=length))
    for kinds in itertools.product(("Absolute", "Or", "And"), repeat=3):
        for grouped in (False, True):
            root = ET.Element("CDXML", BondLength="14.4")
            page = ET.SubElement(root, "page")
            for i, kind in enumerate(kinds):
                part = ET.fromstring(from_mol(Chem.MolFromSmiles("C[C@H](F)Cl"))).find(
                    ".//fragment"
                )
                assert part is not None
                part.set("id", str(10000 + i))
                for node in part.iter():
                    if node.tag != "fragment" and node.get("id") is not None:
                        node.set("id", str(int(node.get("id")) + 10000 * (i + 1)))
                    for key in ("B", "E"):
                        if node.get(key):
                            node.set(key, str(int(node.get(key)) + 10000 * (i + 1)))
                    if node.tag == "n":
                        node.set("EnhancedStereoType", kind)
                        node.set("EnhancedStereoGroupNum", str(i + 1))
                parent = ET.SubElement(page, "group") if grouped and i == 1 else page
                parent.append(part)
            emit(f"combined-groups/{kinds}/{grouped}", root)
    for layer in ("32767", "32768", "-32769", "1" * 100):
        emit("native-layer/" + layer, xml([{}, {}], [(0, 1, dict(Z=layer))]))
    for value in ("-0.1", "1.1", "1e90"):
        root = ET.fromstring(xml([{}, {}], [(0, 1, dict(color="2"))]))
        table = ET.SubElement(root, "colortable")
        ET.SubElement(table, "color", r=value)
        emit("native-color/" + value, root)
        root.find(".//b").set("color", "0")
        emit("unused-wide-color/" + value, root)


if __name__ == "__main__":
    main()
