"""Direct native RDKit CDXML observations.

The reader accepts already flattened/normalized chemical XML. Real drawing
fixtures therefore receive only the same documented bond normalization before
both readers run. Original application checks classify restrictions separately;
expected molecular results always come from the direct native reader.
"""

import itertools
import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

from kekulize_reference import directions
from ranking_reference import metadata
from rdkit import Chem, RDConfig, RDLogger, rdBase
from rdkit.Chem import rdDepictor
from valence_reference import graph

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))


def xml(atoms, bonds=(), *, length="14.4", fragment_id=7000):
    root = ET.Element("CDXML", {} if length is None else {"BondLength": str(length)})
    frag = ET.SubElement(ET.SubElement(root, "page"), "fragment", id=str(fragment_id))
    for i, attrs in enumerate(atoms):
        ET.SubElement(frag, "n", {"id": str(i + 1), "p": f"{i * 14.4} 0", **attrs})
    for i, (a, b, attrs) in enumerate(bonds):
        ET.SubElement(frag, "b", {"id": str(1000 + i), "B": str(a + 1), "E": str(b + 1), **attrs})
    return ET.tostring(root, encoding="unicode")


def observation(mol, source):
    fragment = next(
        f for f in source.iter("fragment") if int(f.get("id", "0")) == mol.GetIntProp("CDX_FRAG_ID")
    )
    atoms = list(fragment.findall("n"))
    bonds = list(fragment.findall("b"))
    conf = mol.GetConformer() if mol.GetNumConformers() else None
    return dict(
        id=mol.GetIntProp("CDX_FRAG_ID"),
        atom_ids=[int(a.get("id", "0")) for a in atoms],
        bond_ids=[int(b.get("id", "0")) for b in bonds],
        fuse_labels=[
            a.GetUnsignedProp("CDX_NODE_ID") if a.HasProp("CDX_NODE_ID") else None
            for a in mol.GetAtoms()
        ],
        graph=graph(mol),
        metadata=metadata(mol),
        directions=directions(mol),
        positions=[
            dict(x=p.x, y=p.y, z=p.z)
            for p in (conf.GetAtomPosition(i) for i in range(mol.GetNumAtoms()))
        ]
        if conf
        else [],
        is_3d=conf.Is3D() if conf else False,
        bond_cfg=[
            b.GetIntProp("_MolFileBondCfg") if b.HasProp("_MolFileBondCfg") else None
            for b in mol.GetBonds()
        ],
        non_explicit_3d_chirality=[
            a.GetIntProp("_NonExplicit3DChirality")
            if a.HasProp("_NonExplicit3DChirality")
            else None
            for a in mol.GetAtoms()
        ],
        # This property is a C++ enum, which Python cannot read as an integer.
        # Observe presence directly; retain the source enum as file annotation.
        bond_cip=[
            {"E": 2, "Z": 3}[source.get("BS")] if b.HasProp("CDX_BOND_CIP") else None
            for b, source in zip(mol.GetBonds(), bonds, strict=True)
        ],
        atom_cip_ranks=[
            a.GetUnsignedProp("_CIPRank") if a.HasProp("_CIPRank") else None for a in mol.GetAtoms()
        ],
    )


def emit(name, text, restriction=None):
    error = None
    expected = None
    native_accepted = False
    try:
        mols = Chem.MolsFromCDXML(text, sanitize=False, removeHs=False)
        native_accepted = True
    except (RuntimeError, ValueError) as exc:
        error = str(exc)
    if native_accepted and not restriction:
        source = ET.fromstring(text)
        expected = dict(fragments=[observation(m, source) for m in mols])
    application_accepted = None
    if restriction:
        from engine import worker

        try:
            worker.handle(dict(protocol=1, operation="import", format="cdxml", text=text))
            application_accepted = True
        except (RuntimeError, ValueError, KeyError):
            application_accepted = False
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                expected=expected,
                failure=error,
                native_accepted=native_accepted,
                restriction=restriction,
                application_accepted=application_accepted,
            )
        )
    )


def from_mol(mol, *, variant=0):
    mol = Chem.Mol(mol)
    rdDepictor.Compute2DCoords(mol)
    Chem.WedgeMolBonds(mol, mol.GetConformer())
    conf = mol.GetConformer()
    nodes = []
    for a in mol.GetAtoms():
        p = conf.GetAtomPosition(a.GetIdx())
        node = dict(Element=str(a.GetAtomicNum()), p=f"{p.x * 9.6:.8f} {-p.y * 9.6:.8f}")
        if a.GetIsotope():
            node["Isotope"] = str(a.GetIsotope())
        if a.GetFormalCharge():
            node["Charge"] = str(a.GetFormalCharge())
        if a.GetNumExplicitHs() or a.GetNoImplicit():
            node["NumHydrogens"] = str(a.GetNumExplicitHs())
        if a.GetNumRadicalElectrons():
            node["Radical"] = "Doublet" if a.GetNumRadicalElectrons() == 1 else "Triplet"
        nodes.append(node)
    bonds = []
    orders = {
        Chem.BondType.SINGLE: "1",
        Chem.BondType.DOUBLE: "2",
        Chem.BondType.TRIPLE: "3",
        Chem.BondType.AROMATIC: "1.5",
        Chem.BondType.DATIVE: "dative",
        Chem.BondType.QUADRUPLE: "4",
    }
    for b in mol.GetBonds():
        attrs = dict(Order=orders[b.GetBondType()])
        a, z = b.GetBeginAtomIdx(), b.GetEndAtomIdx()
        if b.GetBondDir() == Chem.BondDir.BEGINWEDGE:
            attrs["Display"] = "WedgeBegin"
        elif b.GetBondDir() == Chem.BondDir.BEGINDASH:
            attrs["Display"] = "WedgedHashBegin"
        if variant & 1:
            a, z = z, a
            if "Display" in attrs:
                attrs["Display"] = attrs["Display"].replace("Begin", "End")
        bonds.append((a, z, attrs))
    if variant & 2:
        bonds.reverse()
    return xml(nodes, bonds)


def main():
    RDLogger.DisableLog("rdApp.*")
    print(
        json.dumps(dict(rdkit_version=rdBase.rdkitVersion, chemdraw=Chem.HasChemDrawCDXSupport()))
    )
    emit("empty drawing", "<CDXML><page/></CDXML>")
    emit("empty fragment", '<CDXML><page><fragment id="1"/></page></CDXML>')
    for number, charge, hydrogens in itertools.product(range(119), (-2, 0, 2), (None, "0", "2")):
        attrs = dict(
            Element=str(number), Charge=str(charge), Isotope="13" if number % 3 == 0 else "0"
        )
        if hydrogens is not None:
            attrs["NumHydrogens"] = hydrogens
        emit(f"atom/{number}/{charge}/{hydrogens}", xml([attrs]))
    for radical, abnormal, needs_clean in itertools.product(
        ("None", "Singlet", "Doublet", "Triplet"), ("no", "yes", "true"), ("no", "yes")
    ):
        emit(
            f"radical/{radical}/{abnormal}/{needs_clean}",
            xml(
                [
                    dict(
                        Radical=radical,
                        AbnormalValence=abnormal,
                        NeedsClean=needs_clean,
                        NumHydrogens="0",
                    )
                ]
            ),
        )
    displays = (
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
    )
    for order, display in itertools.product(("1", "2", "3", "4", "1.5", "dative"), displays):
        emit(f"bond/{order}/{display}", xml([{}, {}], [(0, 1, dict(Order=order, Display=display))]))
    for length, value in itertools.product(
        (None, "0", "-1", "14.4", "1.23456", "32767.99"),
        ("0", "1.23456", "-0.000015", "-32768", "32767.99"),
    ):
        emit(
            f"coordinates/{length}/{value}",
            xml([dict(p=f"{value} 2.345678"), dict(p="1 -5.123456")], [(0, 1, {})], length=length),
        )
    for kind, group in itertools.product(("Absolute", "Or", "And"), (-1, 0, 1, 2, 127)):
        emit(
            f"enhanced/{kind}/{group}",
            xml(
                [
                    dict(EnhancedStereoType=kind, EnhancedStereoGroupNum=str(group)),
                    dict(EnhancedStereoType="Absolute"),
                    dict(EnhancedStereoType="Or", EnhancedStereoGroupNum="2"),
                ]
            ),
        )
    for display, rotation, mirror, explicit_h in itertools.product(
        displays, range(4), (1, -1), (None, "0", "1")
    ):
        points = [(0, 0), (-14.4, 0), (7.2, 12.470766), (7.2, -12.470766)]
        points = [((-y, x) if rotation & 1 else (x, y)) for x, y in points]
        points = [(x * (-1 if rotation & 2 else 1), y * mirror) for x, y in points]
        atoms = [dict(p=f"{x} {y}", Element=str(n)) for (x, y), n in zip(points, (6, 9, 17, 35))]
        if explicit_h is not None:
            atoms[0]["NumHydrogens"] = explicit_h
        emit(
            f"winding/{display}/{rotation}/{mirror}/{explicit_h}",
            xml(atoms, [(0, 1, dict(Display=display)), (0, 2, {}), (0, 3, {})]),
        )
    for display, bs, shape in itertools.product(("Solid", "Wavy"), (None, "E", "Z"), range(3)):
        atoms = [
            dict(p=p, Element=str(n))
            for p, n in zip(
                ("0 0", "14.4 0", "-7.2 12.47", f"21.6 {12.47 if shape else -12.47}"), (6, 6, 9, 17)
            )
        ]
        attrs = dict(Order="2", Display=display)
        if bs:
            attrs["BS"] = bs
        emit(f"alkene/{display}/{bs}/{shape}", xml(atoms, [(0, 1, attrs), (0, 2, {}), (1, 3, {})]))
    for mirror, center in itertools.product((1, -1), (6, 7, 15, 16, 78)):
        atoms = [dict(Element=str(center), xyz="0 0 0")] + [
            dict(Element=str(n), xyz=f"{x * mirror} {y} {z}")
            for n, (x, y, z) in zip(
                (9, 17, 35, 53), ((1, 1, 1), (1, -1, -1), (-1, 1, -1), (-1, -1, 1))
            )
        ]
        emit(f"3d/{mirror}/{center}", xml(atoms, [(0, i, {}) for i in range(1, 5)]))
    for numbers, is_3d, hydrogens in itertools.product(
        ((6, 6), (6, 8), (6, 7), (0, 0)), (False, True), (None, "0")
    ):
        atoms = [
            dict(Element=str(n), **({"xyz": f"{i} 0 0"} if is_3d else {}))
            for i, n in enumerate(numbers)
        ]
        if hydrogens is not None:
            for atom in atoms:
                atom["NumHydrogens"] = hydrogens
        emit(
            f"bond CIP fallback/{numbers}/{is_3d}/{hydrogens}",
            xml(atoms, [(0, 1, dict(Order="2", BS="E"))]),
        )
    texts = (
        "CCO",
        "c1ccccc1",
        "c1cc[nH]c1",
        "C1CCCCC1",
        "C/C=C/C",
        "F/C=C(/Cl)C=C/F",
        "N[C@@H](C)C(=O)O",
        "C[C@H]1CCCC[C@@H]1O",
        "[2H]O[3H]",
        "[13CH3][NH2+]C",
        "[Na+].[Cl-]",
        "O=N(=O)O",
        "[CH2]C",
        "N->[Pt](<-N)(<-N)<-N",
        "C1=CC=CC=C1C2=CC=CC=C2",
    )
    for smi, variant in itertools.product(texts, range(4)):
        mol = Chem.MolFromSmiles(smi)
        if mol is None:
            raise AssertionError(smi)
        emit(f"molecule/{smi}/{variant}", from_mol(mol, variant=variant))
    emit(
        "group ordering",
        '<CDXML BondLength="14.4"><page><fragment id="20"><n id="21"/></fragment><group id="30"><fragment id="10"><n id="11" Element="8"/></fragment></group><fragment id="40"><n id="41" Element="7"/></fragment></page></CDXML>',
    )
    emit(
        "bonds before nodes",
        '<CDXML BondLength="14.4"><page><fragment id="20"><b id="3" B="1" E="2"/><n id="2" p="14.4 0" Element="8"/><n id="1" p="0 0"/></fragment></page></CDXML>',
    )
    for attrs in (
        dict(NodeType="ExternalConnectionPoint"),
        dict(NodeType="ExternalConnectionPoint", ExternalConnectionType="Diamond"),
        dict(NodeType="GenericNickname", GenericNickname="XH"),
        dict(NodeType="ElementList", ElementList=""),
        dict(NumHydrogens="65535"),
    ):
        emit(f"node semantics/{attrs}", xml([attrs]))
    for node_type in (
        "Unspecified",
        "Element",
        "ElementListNickname",
        "Formula",
        "AnonymousAlternativeGroup",
        "NamedAlternativeGroup",
        "MultiAttachment",
        "VariableAttachment",
        "LinkNode",
        "Monomer",
    ):
        emit(f"native inert node type/{node_type}", xml([dict(NodeType=node_type)]))
    for label in ("R", "R0", "R13", "R13suffix", "R-1", "Rinvalid", "R 15", "R+2", "R1-2", "R++1"):
        source = ET.fromstring(xml([{}]))
        ET.SubElement(ET.SubElement(next(source.iter("n")), "t"), "s").text = label
        emit(f"isotope from label/{label}", ET.tostring(source, encoding="unicode"))
    for label in ("<t>\n  <s>R13</s>\n</t>", "<t>R13</t>", "<t><s>R</s>  <s>13</s></t>"):
        source = ET.fromstring(xml([{}]))
        next(source.iter("n")).append(ET.fromstring(label))
        emit(f"formatted isotope label/{label}", ET.tostring(source, encoding="unicode"))
    for i, line in enumerate(
        (Path(RDConfig.RDDataDir) / "NCI" / "first_5K.smi").read_text().splitlines()[:500]
    ):
        smi = line.split()[0]
        mol = Chem.MolFromSmiles(smi)
        if mol is None:
            raise AssertionError(f"Invalid reference corpus molecule {i}: {smi}")
        for variant in range(4):
            emit(f"NCI/{i}/{variant}", from_mol(mol, variant=variant))
    root = Path(__file__).parent
    for path in sorted((root / "fixtures").glob("*.cdxml")):
        source = ET.fromstring(path.read_text())
        for b in source.iter("b"):
            if b.get("Order") in ("hydrogen", "1.5"):
                b.set("Order", "1")
            display = b.get("Display", "Solid")
            if display.startswith("HollowWedge"):
                b.set("Display", display.replace("HollowWedge", "Wedge"))
            elif display == "Hash":
                b.set("Display", "WedgedHashBegin")
            elif display == "Bold" and b.get("Order", "1") == "1":
                b.set("Display", "WedgeBegin")
        emit(f"fixture/{path.name}", ET.tostring(source, encoding="unicode"))
    for path in sorted((root / "fixtures" / "cdxml-molecular").glob("*.cdxml")):
        restriction = None
        if path.name == "atom-to-fragment.cdxml":
            restriction = "abbreviation expansion precedes molecular parsing; annotation object also outside current drawing contract"
        if path.name == "geometry-tetrahedral-4.cdxml":
            restriction = "annotation presentation object outside current drawing contract"
        emit(f"native fixture/{path.name}", path.read_text(), restriction)
    for attrs in (
        dict(NodeType="GenericNickname", GenericNickname="R1"),
        dict(NodeType="GenericNickname", GenericNickname="A"),
        dict(NodeType="ElementList", ElementList="6 7"),
        dict(RingBondCount="SimpleRing"),
    ):
        emit(f"restriction/{attrs}", xml([attrs]), "queries and unsupported node semantics")
    for order in ("any", "1 2", "5", "hydrogen", "ionic", "0.5"):
        reason = (
            "hydrogen bonds must be normalized to single before this molecular boundary"
            if order == "hydrogen"
            else "outside editable bond contract"
        )
        emit(f"restriction/order/{order}", xml([{}, {}], [(0, 1, dict(Order=order))]), reason)
    emit("normalized hydrogen topology", xml([{}, {}], [(0, 1, dict(Order="1"))]))
    for malformed in (
        "<CDXML>",
        '<CDXML><page><fragment id="1"><n id="2" Charge="junk"/></fragment></page></CDXML>',
        '<CDXML><page><fragment id="1"><n id="2"/><b id="3" B="2" E="4"/></fragment></page></CDXML>',
        '<CDXML><page><fragment id="1"><n id="2"/><n id="2"/></fragment></page></CDXML>',
    ):
        emit(
            "malformed/" + malformed,
            malformed,
            None
            if malformed == "<CDXML>"
            else "malformed chemistry must not be silently coerced or dropped",
        )
    for text in (
        xml([{}], [(0, 0, {})]),
        xml([{}, {}], [(0, 1, {}), (0, 1, {})]),
        "<CDXML><page></CDXML>",
    ):
        emit("native invalid/" + text, text)


if __name__ == "__main__":
    main()
