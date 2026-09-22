"""Complete original import, native base capture, and actual deferred-image wire."""

import ast
import contextlib
import copy
import inspect
import io
import itertools
import json
import math
import random
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from cdxml_molecular_reference import from_mol, xml
from perception_reference import snapshot
from PIL import Image
from rdkit import Chem, RDConfig, RDLogger, rdBase

from engine import worker


def capture_function():
    tree = ast.parse(inspect.getsource(worker.import_cdxml))
    function = tree.body[0]
    assert isinstance(function, ast.FunctionDef)
    body = []
    for statement in function.body:
        if (
            isinstance(statement, ast.Assign)
            and isinstance(statement.targets[0], ast.Name)
            and statement.targets[0].id == "read_text"
        ):
            body.extend(
                ast.parse(
                    "_source_order = list(root.iter())\n_source_ordinals = {el: i for i,el in enumerate(_source_order)}\n_source_parents = {child: parent for parent in root.iter() for child in parent}"
                ).body
            )
        if isinstance(statement, ast.Return):
            body.extend(ast.parse("return locals()").body)
        else:
            body.append(statement)
    function.body = body
    ast.fix_missing_locations(tree)
    namespace = vars(worker).copy()
    exec(compile(tree, "<original import scene>", "exec"), namespace)
    return namespace["import_cdxml"]


CAPTURE = capture_function()


def safe(value):
    if isinstance(value, float) and not math.isfinite(value):
        return None
    if isinstance(value, dict):
        return {k: safe(v) for k, v in value.items()}
    if isinstance(value, list):
        return [safe(v) for v in value]
    return value


def emit(name, text, restriction=None):
    if not isinstance(text, str):
        text = ET.tostring(text, encoding="unicode")
    scene = scene_error = document = document_error = None
    try:
        scope = CAPTURE(text, local_pictures=True)
        root, mol, base = scope["root"], scope["mol"], scope["base"]
        original = scope["_source_ordinals"]
        parents = scope["_source_parents"]
        remaining = set(root.iter())
        base = copy.deepcopy(base)
        for bond in base["bonds"]:
            bond["z_order"] = str(bond["z_order"])
        for graphic in base["graphics"]:
            graphic["layer"] = str(graphic["layer"])
        conf = mol.GetConformer() if mol.GetNumConformers() else None
        scene = dict(
            molecule=dict(
                rdkit_version=rdBase.rdkitVersion,
                ids=list(range(1, mol.GetNumAtoms() + 1)),
                positions=[
                    dict(x=p.x, y=p.y, z=p.z)
                    for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
                ]
                if conf
                else [],
                state=snapshot(mol, "symmetric"),
            ),
            conformer_3d=conf.Is3D() if conf else None,
            base=base,
            objects=[
                dict(source=original[node], atoms=ids) for node, ids in scope["object_map"].items()
            ],
            removed_sources=[
                original[node]
                for node in scope["_source_order"]
                if node not in remaining
                and node.tag == "graphic"
                and parents[node].tag == "fragment"
            ],
        )
    except (ValueError, RuntimeError, KeyError, IndexError, OverflowError, ET.ParseError) as error:
        scene_error = str(error)
    try:
        document = worker.handle(
            dict(protocol=1, operation="import", format="cdxml", text=text, local_pictures=True)
        )["document"]
    except (ValueError, RuntimeError, KeyError, IndexError, OverflowError, ET.ParseError) as error:
        document_error = str(error)
    print(
        json.dumps(
            safe(
                dict(
                    name=name,
                    text=text,
                    scene=scene,
                    scene_error=scene_error,
                    document=document,
                    document_error=document_error,
                    restriction=restriction,
                )
            )
        )
    )


def main():
    RDLogger.DisableLog("rdApp.*")
    print(json.dumps(dict(rdkit_version=rdBase.rdkitVersion)))
    for fixture in sorted(Path(__file__).with_name("fixtures").glob("*.cdxml")):
        emit("fixture/" + fixture.name, fixture.read_text())
    import cdxml_preparation_reference as preparation
    from cdxml_preparation_reference import main as preparation_cases

    old = preparation.emit
    inputs = []
    preparation.emit = lambda name, text, restriction=None, application=False: inputs.append(
        ("preparation/" + name, copy.deepcopy(text), restriction)
    )
    # Collect inputs only; full helper and full handle results below remain
    # independently computed from the complete unchanged original functions.
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            preparation_cases()
    finally:
        preparation.emit = old
    for name, text, restriction in inputs:
        emit(name, text, restriction)
    for root_tag in ("CDXML", "page", "group"):
        for value in ("hello", "αβ ¹²", "a\r\nb", ""):
            root = ET.Element(root_tag)
            page = root if root_tag == "page" else ET.SubElement(root, "page")
            t = ET.SubElement(page, "t", p="1.25 -2.75")
            ET.SubElement(t, "s").text = value
            emit(f"caption-root/{root_tag}/{value!r}", root)
    for flags, size in itertools.product(
        ("0", "1", "2", "4", "32", "64", "96", "3"), ("4", "10", "18.25", "144", "3.9", "nan")
    ):
        root = ET.fromstring(xml([dict(Element="8")]))
        n = root.find(".//n")
        t = ET.SubElement(n, "t", p="0 0")
        ET.SubElement(t, "s", size=size, face=flags).text = "O"
        emit(f"atomic-style/{flags}/{size}", root)
    for attrs in (
        dict(LabelSize="12"),
        dict(LabelFace="1"),
        dict(LabelColor="2"),
        dict(LabelFont="11"),
    ):
        root = ET.fromstring(xml([attrs]))
        emit("synthetic-atom-label/" + str(attrs), root)
    for duplicate in (False, True):
        root = ET.fromstring(xml([dict(Element="8", Charge="1")]))
        page = root.find("page")
        group = ET.SubElement(page, "group", Integral="yes")
        for i in range(3):
            t = ET.SubElement(group, "t", id="100" if duplicate else str(100 + i), p=f"{i * 20} 30")
            ET.SubElement(t, "s").text = f"caption {i}"
        ET.SubElement(
            group, "arrow", id="100", Head3D="40 0 0", Tail3D="0 0 0", ArrowheadHead="Full"
        )
        ET.SubElement(group, "graphic", id="100", GraphicType="Rectangle", BoundingBox="0 50 20 70")
        emit("mixed-group/" + str(duplicate), root)
    for graphic in ("Rectangle", "Oval", "Line", "Bracket", "Symbol"):
        for attrs in (
            dict(),
            dict(RectangleType="RoundEdge"),
            dict(OvalType="Circle"),
            dict(SymbolType="Plus"),
            dict(BracketType="SquarePair"),
        ):
            root = ET.Element("CDXML")
            page = ET.SubElement(root, "page")
            ET.SubElement(page, "graphic", GraphicType=graphic, BoundingBox="0 0 30 20", **attrs)
            emit(f"graphic/{graphic}/{attrs}", root)
    for fmt in ("PNG", "JPEG", "GIF", "BMP", "TIFF"):
        image = Image.new("RGB", (3, 2), (10, 40, 190))
        stream = io.BytesIO()
        image.save(stream, format=fmt)
        root = ET.Element("CDXML")
        page = ET.SubElement(root, "page")
        ET.SubElement(
            page, "embeddedobject", BoundingBox="1 2 31 22", **{fmt: stream.getvalue().hex()}
        )
        emit("picture/" + fmt, root)
    emit("scheme-rejected", "<CDXML><page><scheme/></page></CDXML>")
    emit("step-rejected", '<CDXML><page><step ReactionStepReactants="1"/></page></CDXML>')
    for value in ("nan 0", "inf 0", "0", "bad 0", "0 0 nan", "0 0 0"):
        emit("caption-numeric/" + value, f'<CDXML><page><t p="{value}"><s>x</s></t></page></CDXML>')
    rng = random.Random(18417)
    for i in range(300):
        bits = rng.randrange(0x3F800000, 0x4B000000)
        lo = struct.unpack("<f", struct.pack("<I", bits))[0]
        hi = struct.unpack("<f", struct.pack("<I", bits + 1))[0]
        x = (lo + hi) / 2 / (1 / (14.4 / 42))
        emit(f"caption-midpoint/{i}", f'<CDXML><page><t p="{x} 0"><s>x</s></t></page></CDXML>')
    extra_scene_cases()
    data = Path(RDConfig.RDDataDir) / "NCI" / "first_5K.smi"
    for i, line in enumerate(data.read_text().splitlines()[250:400]):
        mol = Chem.MolFromSmiles(line.split()[0])
        if mol is not None:
            emit(f"nci-extra/{i}", from_mol(mol))


def extra_scene_cases():
    from cdxml_labels_reference import indicator

    # Original aromatic arithmetic determines the independent threshold oracle.
    for smiles in ("c1ccccc1", "c1ccc2ccccc2c1", "c1cc[nH]c1"):
        source = ET.fromstring(from_mol(Chem.MolFromSmiles(smiles)))
        scope = CAPTURE(ET.tostring(source, encoding="unicode"), local_pictures=True)
        mol, base = scope["mol"], scope["base"]
        conf = mol.GetConformer()
        atoms = [
            dict(
                id=i + 1,
                position=dict(x=conf.GetAtomPosition(i).x * 28, y=-conf.GetAtomPosition(i).y * 28),
            )
            for i in range(mol.GetNumAtoms())
        ]
        circles = worker.aromatic.circles(
            dict(atoms=atoms, bonds=base["bonds"], drawing_style=base["drawing_style"]), mol
        )
        assert circles
        center, radius, _ = circles[0]
        threshold = max(1.0, radius * 0.2)
        deltas = [0.0, 0.249999999999, 0.25, 0.250000000001]
        radial = [0.0, math.nextafter(threshold, 0), threshold, math.nextafter(threshold, math.inf)]
        for dx, dr, descendants in itertools.product(deltas, radial, (False, True)):
            root = copy.deepcopy(source)
            page = root.find("page")
            fragment = root.find(".//fragment")
            page.remove(fragment)
            group = ET.SubElement(page, "group", Integral="yes")
            group.append(fragment)
            scale = 1 / (14.4 / 42)
            x, y = center["x"] + dx, center["y"]

            def p(x, y):
                return f"{x / scale!r} {y / scale!r} 0"

            graphic = ET.SubElement(
                fragment,
                "graphic",
                GraphicType="Oval",
                OvalType="Circle",
                Center3D=p(x, y),
                MajorAxisEnd3D=p(x + radius + dr, y),
                MinorAxisEnd3D=p(x, y + radius + dr),
                BoundingBox=f"{(x + radius + dr) / scale} {y / scale} {x / scale} {y / scale}",
            )
            if descendants:
                # This text is read before the owned circle subtree disappears.
                ET.SubElement(ET.SubElement(graphic, "t", p="100 100"), "s").text = "before removal"
            ET.SubElement(fragment, "graphic", GraphicType="Rectangle", BoundingBox="40 40 50 50")
            emit(f"circle-threshold/{smiles}/{dx}/{dr}/{descendants}", root)
    for symbol, charge, radical in itertools.product(
        ("Plus", "Minus", "CirclePlus", "CircleMinus", "Electron", "LonePair", "Radical"),
        (-1, 0, 1),
        (None, "Doublet"),
    ):
        attrs = dict(Element="8", Charge=str(charge), LabelSize="12")
        if radical:
            attrs["Radical"] = radical
        root = ET.fromstring(xml([attrs]))
        fragment = root.find(".//fragment")
        graphic = ET.SubElement(fragment, "graphic", SymbolType=symbol, BoundingBox="4 3 0 0")
        ET.SubElement(
            graphic, "represent", object="1", attribute="Radical" if radical else "Charge"
        )
        root.set("ShowAtomStereo", "yes")
        root.set("ShowAtomNumber", "yes")
        node = root.find(".//n")
        indicator(node, "number", "17", bounds="2 0 6 2")
        indicator(node, "stereo", "R", bounds="0 2 4 6")
        emit(f"styled-mark-label/{symbol}/{charge}/{radical}", root)
    for setting, value in itertools.product(
        (
            "ShowAtomStereo",
            "ShowAtomNumber",
            "ShowNonTerminalCarbonLabels",
            "HideImplicitHydrogens",
        ),
        ("yes", "no", "bad"),
    ):
        root = ET.fromstring(from_mol(Chem.MolFromSmiles("F[C@](Cl)(Br)C/C=C/O")))
        root.set(setting, value)
        root.set("ShowBondStereo", "yes")
        node = root.find(".//n")
        node.set("AtomNumber", "9")
        indicator(node, "number", "n", bounds="1 2 3 4")
        indicator(node, "stereo", "S", bounds="1 2 3 4")
        indicator(root.find(".//b"), "stereo", "E", bounds="1 2 3 4")
        emit(f"label-settings/{setting}/{value}", root)
    # Wide native numbers remain accepted helpers and are rejected only by the
    # complete original Document transport, independently of parser restrictions.
    for layer, color in itertools.product(
        ("-32769", "32768", "999999999999999999999999"), ("0", "1.1", "-0.1")
    ):
        root = ET.fromstring(xml([{}, {}], [(0, 1, dict(Z=layer, color="2"))]))
        colors = ET.SubElement(root, "colortable")
        ET.SubElement(colors, "color", r=color, g="0", b="0")
        emit(f"document-bounds/{layer}/{color}", root)


if __name__ == "__main__":
    main()
