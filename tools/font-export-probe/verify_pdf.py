"""Verify selectable text, embedded instances, PDF widths/bounds and Poppler pixels.

Development dependencies: fonttools==4.61.1, pypdf==6.1.1, Pillow==12.1.1;
pdftoppm and pdftotext must be on PATH. Run after `current fixed` in this directory.
"""

import argparse
import io
import json
import subprocess
from pathlib import Path

from fontTools.pens.boundsPen import BoundsPen
from fontTools.ttLib import TTFont
from PIL import Image, ImageChops
from pypdf import PdfReader

FIXTURES = Path(__file__).resolve().parents[2] / "tests/fixtures/font-export-103"


def widths_of(font):
    result = {}
    items = font["/W"]
    cursor = 0
    while cursor < len(items):
        first = int(items[cursor])
        second = items[cursor + 1]
        if isinstance(second, list):
            result.update((first + i, float(width)) for i, width in enumerate(second))
            cursor += 2
        else:
            last = int(second)
            result.update((i, float(items[cursor + 2])) for i in range(first, last + 1))
            cursor += 3
    return result


def expected_advances(kind, weight):
    name = f"static-{weight}.subset.ttf" if kind == "ttf" else f"cff-static-{weight}.subset.otf"
    font = TTFont(FIXTURES / name)
    cmap = font.getBestCmap()
    return [font["hmtx"][cmap[ord(c)]][0] for c in "HNO"]


def inspect(path, kind, label, variable):
    reader = PdfReader(path)
    assert len(reader.pages) == 1
    page = reader.pages[0]
    expected_text = "HNOHNO" if label == "mixed" else "HNO"
    text = page.extract_text()
    assert "".join(text.split()) == expected_text, (path, text)
    poppler_text = subprocess.check_output(["pdftotext", str(path), "-"], text=True)
    assert "".join(poppler_text.split()) == expected_text, (path, poppler_text)
    fonts = page["/Resources"]["/Font"]
    weights = [400, 700] if label == "mixed" else [int(label)]
    assert len(fonts) == len(weights), (path, fonts)
    names = set()
    programs = set()
    found_weights = []
    details = []
    for reference in fonts.values():
        font = reference.get_object()
        assert font["/Subtype"] == "/Type0"
        assert font["/ToUnicode"].get_data()
        names.add(str(font["/BaseFont"]))
        descendant = font["/DescendantFonts"][0].get_object()
        descriptor = descendant["/FontDescriptor"]
        widths = widths_of(descendant)
        actual = [widths[i] for i in [1, 2, 3]]
        matching = [w for w in weights if actual == expected_advances(kind, w)]
        assert len(matching) == 1, (path, actual, weights)
        found_weights.extend(matching)
        should_be_truetype = variable or kind == "ttf"
        if should_be_truetype:
            assert descendant["/Subtype"] == "/CIDFontType2"
            assert descendant["/CIDToGIDMap"] == "/Identity"
            assert "/FontFile2" in descriptor and "/FontFile3" not in descriptor
            data = descriptor["/FontFile2"].get_data()
            embedded = TTFont(io.BytesIO(data))
            assert "glyf" in embedded and "CFF2" not in embedded and "fvar" not in embedded
            order = embedded.getGlyphOrder()
            assert [embedded["hmtx"][order[i]][0] for i in [1, 2, 3]] == actual
            glyphs = embedded.getGlyphSet()
            bbox = [float(v) for v in descriptor["/FontBBox"]]
            for name in order:
                pen = BoundsPen(glyphs)
                glyphs[name].draw(pen)
                if pen.bounds:
                    lo_x, lo_y, hi_x, hi_y = pen.bounds
                    # Fixture UPEM is 1000, the same scale as the PDF descriptor.
                    assert bbox[0] <= lo_x and bbox[1] <= lo_y, (path, bbox, pen.bounds)
                    assert bbox[2] >= hi_x and bbox[3] >= hi_y, (path, bbox, pen.bounds)
        else:
            assert descendant["/Subtype"] == "/CIDFontType0"
            assert "/FontFile3" in descriptor and "/FontFile2" not in descriptor
            stream = descriptor["/FontFile3"]
            assert stream["/Subtype"] == "/CIDFontType0C"
            data = stream.get_data()
        programs.add(data)
        details.append(
            {"weight": matching[0], "advances": actual, "subtype": str(descendant["/Subtype"])}
        )
    assert sorted(found_weights) == weights
    assert len(names) == len(weights) and len(programs) == len(weights)
    return {"file": path.name, "text": text, "fonts": details}


def raster_difference(variable, reference):
    for path in [variable, reference]:
        subprocess.run(
            ["pdftoppm", "-r", "144", "-singlefile", "-png", str(path), str(path.with_suffix(""))],
            check=True,
            capture_output=True,
        )
    actual = Image.open(variable.with_suffix(".png")).convert("RGB")
    expected = Image.open(reference.with_suffix(".png")).convert("RGB")
    assert actual.size == expected.size
    difference = ImageChops.difference(actual, expected)
    changed = sum(sum(pixel) for pixel in difference.get_flattened_data())
    coverage = sum(sum(255 - value for value in pixel) for pixel in expected.get_flattened_data())
    ratio = changed / max(coverage, 1)
    # CFF2->TrueType approximates cubics; static CFF1 also rounds blend deltas.
    # The separate unrounded outline oracle prevents this bound hiding wrong weights.
    assert ratio <= 0.02, (variable, ratio)
    return ratio


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    results = []
    for kind in ["ttf", "cff2"]:
        for label in ["400", "700", "mixed"]:
            variable = args.directory / f"{kind}-{label}-variable.pdf"
            reference = args.directory / f"{kind}-{label}-static.pdf"
            results.append(inspect(variable, kind, label, True))
            results.append(inspect(reference, kind, label, False))
            results[-2]["poppler_pixel_difference_ratio"] = raster_difference(variable, reference)
    result = {"status": "passed", "pdfs": results}
    (args.directory / "pdf-report.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
