"""Compare the original flatten() tree/metadata without numeric tolerances.

Ambiguous duplicate IDs or discarded nested chemistry are separate restrictions
even when the old application accepts them by dropping atoms. Internal DTD
declarations are also explicitly restricted; ElementTree applies ATTLIST
defaults but roxmltree does not. Their original/app acceptances remain recorded.
"""

import copy
import itertools
import json
import random
import sys
import unicodedata
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from engine import abbreviations_exchange, cdx_exchange
from engine.abbreviations import PRESETS
from engine.worker import handle


def canonical(node):
    return dict(
        tag=node.tag,
        attributes=node.attrib,
        text=node.text or "",
        tail=node.tail or "",
        children=[canonical(c) for c in node],
    )


def drawing(*, attached=True, connection=True, label="OMe"):
    root = ET.Element("CDXML", BondLength="14.4")
    parent = ET.SubElement(ET.SubElement(root, "page", id="1"), "fragment", id="2")
    ET.SubElement(parent, "n", id="3", p="-14.4 0")
    outer = ET.SubElement(parent, "n", id="4", p="0 0", NodeType="Fragment")
    inner = ET.SubElement(outer, "fragment", id="5")
    a = ET.SubElement(inner, "n", id="6", p="0 0", Element="8")
    ET.SubElement(ET.SubElement(a, "t", p="1 2"), "s", font="3").text = "O"
    ET.SubElement(inner, "n", id="7", p="14.4 0")
    ET.SubElement(inner, "b", id="8", B="6", E="7", Order="1")
    if connection:
        ET.SubElement(inner, "n", id="9", p="-14.4 0", NodeType="ExternalConnectionPoint")
        ET.SubElement(inner, "b", id="10", B="9", E="6", Order="1")
    text = ET.SubElement(outer, "t", p="0 0", LabelAlignment="Right")
    ET.SubElement(text, "s", font="3", size="10", face="96").text = label
    if attached:
        ET.SubElement(parent, "b", id="11", B="3", E="4", Order="1", Display="WedgeBegin")
    return root


def named(root, identifier):
    return next(n for n in root.iter() if n.get("id") == str(identifier))


def mutate(root, identifier, **attrs):
    named(root, identifier).attrib.update(attrs)
    return root


def remove(root, identifier):
    node = named(root, identifier)
    next(p for p in root.iter() if node in list(p)).remove(node)
    return root


def cases():
    for attached, connection in itertools.product((False, True), repeat=2):
        yield (
            f"attachment-{attached}-{connection}",
            drawing(attached=attached, connection=connection),
        )
    for name, (_, reverse) in PRESETS.items():
        for label in {name, reverse} - {""}:
            for split in range(len(label) + 1):
                root = drawing(label=label)
                text = named(root, 4).find("t")
                text[0].text = label[:split]
                ET.SubElement(text, "s", font="5", color="2", face="1").text = label[split:]
                yield f"styled-{label}-{split}", root
    for label in ("X", "", "CO₂R", 'α&β<γ>"', " OMe ", "中文", "Me\rO"):
        yield f"custom-{label!r}", drawing(label=label)
    root = drawing()
    label = named(root, 4).find("t")
    label.text = "not a span"
    label[0].text = "O"
    ET.SubElement(label[0], "s").text = "ignored nested span"
    label[0][0].tail = "ignored tail"
    label[0].tail = "ignored direct tail"
    ET.SubElement(label, "s").text = "Me"
    yield "text-and-tail-reading-order", root
    root = drawing()
    for n in root.iter():
        n.tail = "\n "
    named(root, 6).set("Custom", "<>&\"'\t\n\r")
    yield "escaped-attributes-and-tails", root
    for kind in ("Fragment", "Nickname"):
        for side in ("B", "E"):
            for order in ("1", "2", "3", "1.5", "dative", "hydrogen"):
                root = mutate(drawing(), 4, NodeType=kind)
                mutate(root, 10, Order=order)
                mutate(root, 11, Order=order)
                if side == "B":
                    mutate(root, 11, B="4", E="3")
                    mutate(root, 10, B="6", E="9")
                yield f"direction-{kind}-{side}-{order}", root
    rng = random.Random(764393)
    for i in range(4000):
        root = drawing()
        for identifier in (3, 4, 6, 7, 9):
            p = [rng.uniform(-500, 500) for _ in range(2)]
            mutate(root, identifier, p=" ".join(map(repr, p)))
        yield f"geometry-{i}", root
    values = (
        "0",
        "-0",
        "1e-20",
        "1e-7",
        "9.99999996e-5",
        "0.0001",
        "1e-6",
        "-1e-6",
        "99999999.5",
        "1e20",
        "1e308",
        "5e-324",
        "1e-999",
        "1_4.4",
        "１４.４",
        "١٤.٤",
    )
    for value, identifier in itertools.product(values, (3, 4, 6, 7, 9)):
        root = mutate(drawing(), identifier, p=f"{value} {value}")
        yield f"numeric-{identifier}-{value}", root
    assert unicodedata.unidata_version == "15.0.0"
    for code in range(0x110000):
        if unicodedata.category(chr(code)) == "Nd" and unicodedata.decimal(chr(code)) == 0:
            digits = chr(code + 1) + "_" + chr(code + 4) + "." + chr(code + 4)
            yield f"unicode-decimal-{code:x}", mutate(drawing(), 7, p=f"{digits} {digits}")
    for identifier, value in itertools.product(
        (3, 4, 6, 7, 9), ("nan", "inf", "-inf", "NaN", "1e999")
    ):
        yield f"nonfinite-{identifier}-{value}", mutate(drawing(), identifier, p=f"{value} 0")
    # Sequential wrappers attached to one another use the previous expansion's
    # restored anchor when the second wrapper is processed.
    root = drawing()
    other = copy.deepcopy(named(root, 4))
    for element in other.iter():
        for key in ("id", "B", "E"):
            if key in element.attrib:
                element.set(key, str(int(element.get(key)) + 100))
    other.set("p", "50 10")
    named(root, 2).append(other)
    remove(root, 3)
    mutate(root, 11, B="104")
    yield "two-connected-wrappers", root
    root = drawing()
    parent = named(root, 2)
    parent[:] = list(reversed(parent))
    named(root, 5)[:] = list(reversed(named(root, 5)))
    named(root, 4)[:] = list(reversed(named(root, 4)))
    yield "reversed-child-order", root
    # Original explicit failures.
    yield "missing-definition", remove(drawing(), 5)
    root = drawing()
    named(root, 4).remove(named(root, 4).find("t"))
    yield "missing-label", root
    root = drawing()
    named(root, 4).append(ET.Element("fragment"))
    yield "two-definitions", root
    yield "nested", mutate(drawing(), 7, NodeType="Fragment")
    root = drawing()
    ET.SubElement(named(root, 5), "graphic", id="20")
    yield "unsupported-definition-object", root
    yield "two-connections", mutate(drawing(), 7, NodeType="ExternalConnectionPoint")
    root = drawing()
    root[0].append(named(root, 4))
    named(root, 2).remove(named(root, 4))
    yield "outside-fragment", root
    root = drawing()
    ET.SubElement(named(root, 2), "b", id="20", B="4", E="3")
    yield "two-external-bonds", root
    yield "no-connection-bond", remove(drawing(), 10)
    root = drawing()
    ET.SubElement(named(root, 5), "b", id="20", B="9", E="7")
    yield "two-connection-bonds", root
    yield "missing-anchor", mutate(drawing(), 10, E="999")
    yield "order-conflict", mutate(drawing(), 10, Order="2")
    root = drawing(attached=False, connection=False)
    named(root, 5)[:] = []
    yield "empty-definition", root
    yield "missing-neighbor", remove(drawing(), 3)
    for identifier, value in itertools.product(
        (3, 4, 6, 7, 9), ("", "0", "1 2 3", "bad 2", "_1 0", "1__0 0")
    ):
        yield f"invalid-position-{identifier}-{value}", mutate(drawing(), identifier, p=value)
    root = drawing(attached=False)
    mutate(root, 9, p="ignored bad connection geometry")
    yield "disconnected-source-geometry-ignored", root
    root = drawing()
    ET.SubElement(named(root, 7), "t", p="400 500").text = "retained child label"
    yield "member-label-repositioned", root
    # Atomic failure after a prior successful mutation.
    root = drawing()
    other = copy.deepcopy(named(root, 4))
    other.set("id", "104")
    other.set("p", "bad")
    named(root, 2).append(other)
    yield "second-wrapper-failure", root
    root = drawing()
    # The initial wrapper snapshot contains a definition later discarded with
    # its containing text. It must not be expanded from an unreachable node.
    other = copy.deepcopy(named(root, 4))
    other.set("id", "104")
    ET.SubElement(named(root, 4).find("t"), "fragment", id="105").append(other)
    yield "discarded-wrapper-in-label", root
    fixture = Path(__file__).parent / "fixtures/abbreviations-native.cdx"
    yield "native-binary-abbreviations", ET.fromstring(cdx_exchange.from_cdx(fixture.read_bytes()))
    for path in sorted((Path(__file__).parent / "fixtures").glob("*.cdxml")):
        yield f"drawing-{path.name}", ET.parse(path).getroot()
    # The existing application writer produces all preset chemical definitions.
    original = handle(dict(protocol=1, operation="import", format="smiles", text="CCC"))["document"]
    for label in PRESETS:
        changed = handle(
            dict(
                protocol=1,
                operation="abbreviate",
                format="replace",
                document=original,
                selected_ids=[original["atoms"][-1]["id"]],
                text=label,
            )
        )["document"]
        xml = handle(dict(protocol=1, operation="export", format="cdxml", document=changed))[
            "output"
        ]
        yield f"application-export-{label}", ET.fromstring(xml)


def emit(
    name,
    text,
    *,
    restriction=None,
    allowed_application_acceptance=False,
    application_expectation=None,
):
    if not isinstance(text, str):
        text = ET.tostring(text, encoding="unicode")
    expected = error = None
    try:
        root = ET.fromstring(text)
        records = abbreviations_exchange.flatten(root)
        expected = dict(tree=canonical(root), abbreviations=records)
    except (ValueError, KeyError, TypeError) as exc:
        error = str(exc)
    application_accepted = None
    if restriction or application_expectation is not None:
        try:
            handle(dict(protocol=1, operation="import", format="cdxml", text=text))
            application_accepted = True
        except (ValueError, KeyError, TypeError, RuntimeError):
            application_accepted = False
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                expected=expected,
                error=error,
                restriction=restriction,
                application_accepted=application_accepted,
                allowed_application_acceptance=allowed_application_acceptance,
                application_expectation=application_expectation,
            ),
            ensure_ascii=True,
        )
    )


def main():
    for name, root in cases():
        emit(name, root)
    for identifier in (6, 7, 9):
        root = drawing()
        del named(root, identifier).attrib["id"]
        emit(f"missing-id-{identifier}", root)
    root = drawing(attached=False, connection=False)
    remove(root, 7)
    remove(root, 8)
    del named(root, 6).attrib["id"]
    emit("single-missing-id", root, application_expectation=True)
    root = drawing()
    mutate(root, 7, id="6")
    emit("duplicate-id", root, restriction="Duplicate abbreviation atom ID")
    root = drawing(attached=False, connection=False)
    remove(root, 8)
    mutate(root, 7, id="6")
    emit(
        "duplicate-id-drops-oxygen",
        root,
        restriction="Duplicate abbreviation atom ID",
        allowed_application_acceptance=True,
    )
    root = drawing()
    ET.SubElement(
        ET.SubElement(named(root, 5), "fragment", id="500"), "n", id="501", p="50 50", Element="7"
    )
    emit(
        "discarded-nested-chemistry",
        root,
        restriction="Discarded abbreviation chemistry",
        allowed_application_acceptance=True,
    )
    root = drawing()
    ET.SubElement(named(root, 6).find("t"), "n", id="501", p="50 50", Element="7")
    emit(
        "discarded-anchor-label-chemistry",
        root,
        restriction="Discarded abbreviation chemistry",
        allowed_application_acceptance=True,
    )
    emit(
        "connection-self-loop",
        mutate(drawing(), 10, E="9"),
        restriction="Invalid abbreviation connection point",
    )
    emit(
        "external-doctype", '<!DOCTYPE CDXML SYSTEM "http://invalid.example/no-fetch.dtd"><CDXML/>'
    )
    emit("comments-and-pis", "<CDXML>one<!-- comment -->two<?ignore data?>three<page/>four</CDXML>")
    emit("namespace", '<CDXML xmlns="urn:example"/>', restriction="Unsupported namespace")
    body = '<CDXML BondLength="14.4"><page><fragment id="1"><n id="2" p="0 0"/><n id="3" p="14.4 0"/><b id="4" B="2" E="3"/></fragment></page></CDXML>'
    for declaration in (
        '<!ENTITY x "ignored">',
        '<!ATTLIST CDXML BondLength CDATA "14.4">',
        "<!ELEMENT CDXML ANY>",
        '<!NOTATION png SYSTEM "image/png">',
    ):
        emit(
            "internal-declaration-" + declaration,
            "<!DOCTYPE CDXML ["
            + declaration
            + "]>"
            + (
                body.replace(' BondLength="14.4"', "")
                if declaration.startswith("<!ATTLIST")
                else body
            ),
            restriction="Internal DTD declarations",
            allowed_application_acceptance=True,
            application_expectation=True,
        )
    emit(
        "inert-declaration-text",
        '<!-- <!ENTITY fake "text"> --><?ignore <!ATTLIST?><CDXML><page><t><![CDATA[<!ELEMENT example>]]></t></page></CDXML>',
    )
    emit("empty-internal-subset", '<!DOCTYPE CDXML [<!-- <!ENTITY fake "text"> -->]><CDXML/>')


if __name__ == "__main__":
    main()
