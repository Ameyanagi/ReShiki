"""Original worker chemistry_xml observations, plus direct native parsing.

All style/order combinations are compared as XML trees with ordered attributes.
Unsupported XML namespaces, root names, and internal DTD declarations are
reported separately from matches. Supported normalized orders additionally
compare directly with the pinned ChemDraw-enabled RDKit reader.
"""

import itertools
import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from cdxml_molecular_reference import observation
from rdkit import Chem, RDLogger, rdBase

from engine import bonds_exchange
from engine.worker import chemistry_xml, handle


def canonical(node):
    return dict(
        tag=node.tag,
        attributes=list(node.attrib.items()),
        text=node.text or "",
        tail=node.tail or "",
        children=[canonical(child) for child in node],
    )


def drawing(order=None, display=None, secondary=None):
    root = ET.Element("CDXML", BondLength="14.4")
    fragment = ET.SubElement(ET.SubElement(root, "page"), "fragment", id="1")
    ET.SubElement(fragment, "n", id="2", p="0 0")
    ET.SubElement(fragment, "n", id="3", p="14.4 0")
    attrs = {"id": "4", "B": "2", "E": "3"}
    for key, value in (("Display2", secondary), ("Order", order), ("Display", display)):
        if value is not None:
            attrs[key] = value
    ET.SubElement(fragment, "b", attrs)
    return root


def emit(name, root, *, restriction=None, native=False, application=False):
    text = root if isinstance(root, str) else ET.tostring(root, encoding="unicode")
    expected = failure = native_expected = native_failure = application_order = None
    try:
        source = ET.fromstring(text)
        before = canonical(source)
        normalized = chemistry_xml(source)
        assert canonical(source) == before
        expected = canonical(normalized)
        if native:
            try:
                parts = Chem.MolsFromCDXML(
                    ET.tostring(normalized, encoding="unicode"), sanitize=False, removeHs=False
                )
                native_expected = dict(fragments=[observation(part, normalized) for part in parts])
            except (ValueError, RuntimeError) as exc:
                native_failure = str(exc)
    except (ValueError, KeyError, TypeError) as exc:
        failure = str(exc)
    if application:
        response = handle(dict(protocol=1, operation="import", format="cdxml", text=text))
        application_order = response["document"]["bonds"][0]["order"]
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                expected=expected,
                failure=failure,
                restriction=restriction,
                native=native and expected is not None,
                native_expected=native_expected,
                native_failure=native_failure,
                application_order=application_order,
            )
        )
    )


def main():
    assert chemistry_xml is bonds_exchange.chemistry_xml
    print(
        json.dumps(dict(rdkit_version=rdBase.rdkitVersion, chemdraw=Chem.HasChemDrawCDXSupport()))
    )
    primary = (None, *bonds_exchange.DISPLAY, "", "solid", "DottedHydrogen", "HollowWedge", "Dash ")
    secondary = (None, *bonds_exchange.DISPLAY, "DottedHydrogen", "", "solid", "Dash ")
    orders = (
        None,
        "1",
        "2",
        "3",
        "4",
        "1.5",
        "hydrogen",
        "dative",
        "0",
        "1.0",
        "aromatic",
        "2.5",
        "2 3",
        "",
        "1 ",
        "65535",
        "any",
    )
    native_orders = {None, "1", "2", "3", "4", "1.5", "hydrogen", "dative"}
    for order, display, second in itertools.product(orders, primary, secondary):
        emit(
            f"cross-{order!r}-{display!r}-{second!r}",
            drawing(order, display, second),
            native=order in native_orders,
        )
    for display in ("HollowWedgeBegin", "HollowWedgeEnd", "Hash", "Bold"):
        root = drawing("1.5", display, "DottedHydrogen")
        for node in root.iter():
            node.text = "\n    "
            node.tail = "\n "
        root[0][0][-1].set("Unknown", "a&b<\"'\t\n\r")
        emit(f"ordered-attributes-and-text-{display}", root, native=True)
    # Picture contents are not chemistry. Invalid displays inside any removed
    # subtree must not affect the surviving molecule or trigger native parsing.
    for placement in ("page", "fragment", "group", "nested", "between-atoms"):
        root = drawing("hydrogen", "Bold", "DottedHydrogen")
        page, fragment = root[0], root[0][0]
        parent = page if placement == "page" else fragment
        if placement in ("group", "nested"):
            parent = ET.SubElement(page, "group", id="100")
        picture = ET.SubElement(parent, "embeddedobject", id="101", PNG="not decoded")
        picture.text = "binary representation is deliberately not parsed"
        picture.tail = "discarded picture tail"
        if placement == "nested":
            picture = ET.SubElement(picture, "embeddedobject", id="102")
        ET.SubElement(picture, "b", id="103", Display="Unsupported", Display2="Unsupported")
        if placement == "between-atoms":
            parent.remove(picture)
            parent.insert(1, picture)
        emit(f"picture-{placement}", root, native=True)
    root = drawing("1", "Solid", "Solid")
    fragment = root[0][0]
    for i in range(30):
        image = ET.SubElement(fragment, "embeddedobject", id=str(100 + i))
        image.tail = f"tail {i}"
    emit("consecutive-pictures", root, native=True)
    root = drawing("1", "Solid", "Solid")
    root[0].set("Display", "invalid inherited display is ignored")
    root[0].set("Display2", "invalid inherited secondary is ignored")
    emit("display-is-not-inherited", root, native=True)
    root = drawing("1", "HollowWedgeBegin", "DottedHydrogen")
    ET.SubElement(root[0][0], "b", id="100", Display="invalid")
    emit("failure-after-previous-bond-edits", root)
    for path in sorted((Path(__file__).parent / "fixtures").glob("*.cdxml")):
        emit(f"drawing-{path.name}", ET.parse(path).getroot())
    emit("raw-hydrogen-application", drawing("hydrogen"), native=True, application=True)
    emit(
        "display2-hydrogen-application",
        drawing("1", "Dash", "DottedHydrogen"),
        native=True,
        application=True,
    )
    emit(
        "external-doctype",
        '<!DOCTYPE CDXML SYSTEM "http://invalid.example/no-fetch.dtd">'
        + ET.tostring(drawing(), encoding="unicode"),
        native=True,
    )
    emit(
        "inert-declarations",
        '<!-- <!ENTITY x "inert"> --><?inert data?><CDXML><page><t><![CDATA[<!ATTLIST sample>]]></t></page></CDXML>',
    )
    for namespace in ('xmlns="urn:example"', 'xmlns:x="urn:example" x:id="1"'):
        emit(
            "namespace-" + namespace,
            "<CDXML " + namespace + "><page/></CDXML>",
            restriction="namespace",
        )
    emit(
        "removed-picture-namespace",
        '<CDXML><page><embeddedobject xmlns="urn:picture"/></page></CDXML>',
        restriction="namespace",
    )
    for declaration in (
        '<!ENTITY x "text">',
        '<!ATTLIST CDXML BondLength CDATA "14.4">',
        "<!ELEMENT CDXML ANY>",
        '<!NOTATION image SYSTEM "image/png">',
    ):
        emit(
            "dtd-" + declaration,
            "<!DOCTYPE CDXML [" + declaration + "]><CDXML><page/></CDXML>",
            restriction="internal DTD",
        )
    for tag in ("page", "embeddedobject"):
        emit("non-CDXML-root-" + tag, f'<{tag}><b Order="hydrogen"/></{tag}>', restriction="root")


if __name__ == "__main__":
    RDLogger.DisableLog("rdApp.warning")
    RDLogger.DisableLog("rdApp.error")
    main()
