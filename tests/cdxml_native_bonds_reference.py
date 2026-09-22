"""Original read_bonds, retaining wide native channels and decimal layers."""

import copy
import itertools
import json
import xml.etree.ElementTree as ET

import cdxml_bonds_reference as original

from engine.bonds_exchange import read_bonds


def emit(
    name,
    root,
    parts=None,
    scale=original.SCALE,
    colors=original.COLORS,
    parsed_xml=None,
    source=None,
):
    del parsed_xml
    parts = [original.molecule()] if parts is None else parts
    text = ET.tostring(root, encoding="unicode") if source is None else source
    before = ET.tostring(root)
    observed = [original.snapshot(part) for part in parts]
    expected = document = failure = None
    try:
        expected = read_bonds(root, parts, scale, colors)
        if all(
            -32768 <= bond["z_order"] <= 32767
            and all(
                isinstance(c, (int, float)) and c == int(c) and 0 <= c <= 255 for c in bond["color"]
            )
            for bond in expected
        ):
            document = copy.deepcopy(expected)
        for bond in expected:
            bond["z_order"] = str(bond["z_order"])
            bond["color"] = [float(c) for c in bond["color"]]
    except (ValueError, KeyError, RuntimeError) as error:
        failure = str(error)
    assert ET.tostring(root) == before
    assert observed == [original.snapshot(part) for part in parts]
    print(
        json.dumps(
            dict(
                name=name,
                text=text,
                parts=observed,
                scale=scale,
                colors=colors,
                expected=expected,
                document=document,
                failure=failure,
            )
        )
    )


def main():
    original.emit = emit
    original.main()
    for layer, channel in itertools.product(
        (
            "0",
            "-0",
            "+０００_１２",
            "-32768",
            "32767",
            "32768",
            "-32769",
            "1" * 100,
            "9" * 4300,
            "-" + "9" * 4300,
            "0" * 4301,
            "1_",
            "bad",
        ),
        (-25, 0, 255, 280, 1e90),
    ):
        root = original.drawing(dict(Z=layer, color="0"))
        emit(f"wide/{layer[:20]}/{len(layer)}/{channel}", root, colors=[[channel, 0, 255]])
    for layer in ("-" + "0" * 300 + "12", "+" + "０_" * 300 + "３"):
        emit("canonical/" + layer[:20], original.drawing(dict(Z=layer)))


if __name__ == "__main__":
    main()
