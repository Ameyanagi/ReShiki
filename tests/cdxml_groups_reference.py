"""Direct original read_groups oracle, with identity-based mappings."""

import itertools
import json
import random
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine.groups_exchange import read_groups


def emit(name, text, objects, first=100, restriction=None):
    expected = failure = None
    try:
        root = ET.fromstring(text)
        source = ET.tostring(root)
        nodes = list(root.iter())
        mappings = {nodes[item["source"]]: item["atoms"] for item in objects}
        before = {node: list(ids) for node, ids in mappings.items()}
        expected = read_groups(root, mappings, first)
        assert before == mappings and source == ET.tostring(root)
    except (ValueError, IndexError, ET.ParseError) as exc:
        failure = str(exc)
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                objects=objects,
                first=first,
                expected=expected,
                failure=failure,
                restriction=restriction,
            )
        )
    )


def mapping(source, *ids):
    return dict(source=source, atoms=list(ids))


def main():
    for integral, outer, inner in itertools.product(
        (None, "yes", "no", "YES", "", "bad"),
        (None, [], [9], [9, 4], [4, 4, 9, 2]),
        (None, [], [4], [4, 9]),
    ):
        text = (
            "<CDXML><group><group"
            + (f' Integral="{integral}"' if integral is not None else "")
            + "><n/><n/></group><t/></group></CDXML>"
        )
        objects = [mapping(3, 1), mapping(4, 2), mapping(5, 3)]
        if outer is not None:
            objects.append(mapping(1, *outer))
        if inner is not None:
            objects.append(mapping(2, *inner))
        emit(f"mapped-parent-{integral}-{outer}-{inner}", text, objects)
    emit("empty", "<CDXML/>", [])
    emit("groups-without-mapping", "<CDXML><group><group><n/></group></group></CDXML>", [])
    emit(
        "last-map-wins",
        "<CDXML><group><n/><n/></group></CDXML>",
        [mapping(2, 1), mapping(3, 2), mapping(2, 3, 4)],
    )
    emit(
        "duplicate-xml-ids",
        '<CDXML><group id="same"><n id="same"/><n id="same"/></group><group id="same"><n id="same"/><n id="same"/></group></CDXML>',
        [mapping(2, 7), mapping(3, 2), mapping(5, 9), mapping(6, 1)],
    )
    emit(
        "nested-redundancy-integral",
        '<CDXML><group Integral="yes"><group><n/><n/></group></group><group><n/><n/></group></CDXML>',
        [mapping(3, 1), mapping(4, 2), mapping(6, 2), mapping(7, 1)],
    )
    emit(
        "empty-map-blocks-descendants",
        "<CDXML><group><group><n/><n/></group><t/></group></CDXML>",
        [mapping(2), mapping(3, 1), mapping(4, 2), mapping(5, 3)],
    )
    emit(
        "root-map-does-not-block-group-scan",
        "<CDXML><group><n/><n/></group></CDXML>",
        [mapping(0), mapping(2, 1), mapping(3, 2)],
    )
    emit("invalid-source-ordinal", "<CDXML/>", [mapping(1, 2)])
    emit("max-identifier", "<CDXML><group/></CDXML>", [mapping(1, 0, 2**64 - 1)], first=2**64 - 1)
    emit(
        "overflow-restriction",
        "<CDXML><group/><group/></CDXML>",
        [mapping(1, 1, 2), mapping(2, 3, 4)],
        first=2**64 - 1,
        restriction="Result group ID exceeds u64",
    )
    for declaration in (
        '<!DOCTYPE CDXML [<!ATTLIST group Integral CDATA "yes">]>',
        '<!DOCTYPE CDXML [<!ENTITY text "hello">]>',
    ):
        emit(
            "dtd-restriction",
            declaration + "<CDXML><group><n/><n/></group></CDXML>",
            [mapping(2, 1), mapping(3, 2)],
            restriction="Internal DTD",
        )
    emit(
        "namespace-restriction",
        '<CDXML xmlns:x="urn:x" x:test="1"><group><n/><n/></group></CDXML>',
        [mapping(2, 1), mapping(3, 2)],
        restriction="Namespace",
    )
    rng = random.Random(313)
    for case in range(500):
        root = ET.Element("CDXML")
        containers = [root]
        for _ in range(rng.randrange(2, 50)):
            parent = rng.choice(containers)
            tag = rng.choice(("group", "group", "fragment", "n", "t", "graphic"))
            node = ET.SubElement(
                parent, tag, dict(id="same", Integral=rng.choice(("yes", "no", "bad")))
            )
            if tag in ("group", "fragment"):
                containers.append(node)
        objects = [
            mapping(i, *[rng.randrange(1, 50) for _ in range(rng.randrange(5))])
            for i, _ in enumerate(root.iter())
            if rng.random() < 0.4
        ]
        emit(
            f"random-tree-{case}",
            ET.tostring(root, encoding="unicode"),
            objects,
            first=rng.randrange(1000),
        )


if __name__ == "__main__":
    main()
