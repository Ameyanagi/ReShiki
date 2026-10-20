#!/usr/bin/env python3
"""Measure exact-color frame strokes in PNG screenshots, without third-party libs.

Coordinates are zero-based screenshot pixels. Scan only the configured canvas.
Rows/columns must contain >400 exact RGB matches, so compact resize handles do
not extend the measured rectangle. Consecutive qualifying coordinates form one
    stroke. A rectangle is reported only when two horizontal and two vertical
    strokes are unambiguous. Geometric dimensions use corresponding stroke-edge
    distances; the raw outer footprint includes both outermost stroke pixels.
"""

import argparse
import glob
import json
import struct
import zlib
from collections import Counter
from pathlib import Path


def png_rows(path):
    data = Path(path).read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("Not a PNG")
    offset, idat, palette = 8, [], None
    while offset < len(data):
        length = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4 : offset + 8]
        body = data[offset + 8 : offset + 8 + length]
        if kind == b"IHDR":
            width, height, depth, color_type, comp, filt, interlace = struct.unpack(
                ">IIBBBBB", body
            )
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"PLTE":
            palette = [tuple(body[i : i + 3]) for i in range(0, len(body), 3)]
        elif kind == b"IEND":
            break
        offset += length + 12
    if depth != 8 or interlace or comp or filt:
        raise ValueError(f"Unsupported PNG depth/interlace/compression/filter: {depth}/{interlace}/{comp}/{filt}")
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}.get(color_type)
    if channels is None:
        raise ValueError(f"Unsupported PNG color type {color_type}")
    raw = zlib.decompress(b"".join(idat))
    stride = width * channels
    if len(raw) != height * (stride + 1):
        raise ValueError("Unexpected decompressed PNG size")
    rows, previous = [], bytearray(stride)
    for y in range(height):
        pos = y * (stride + 1)
        method, row = raw[pos], bytearray(raw[pos + 1 : pos + 1 + stride])
        if method == 1:
            for i in range(channels, stride):
                row[i] = (row[i] + row[i - channels]) & 255
        elif method == 2:
            for i in range(stride):
                row[i] = (row[i] + previous[i]) & 255
        elif method == 3:
            for i in range(stride):
                left = row[i - channels] if i >= channels else 0
                row[i] = (row[i] + ((left + previous[i]) // 2)) & 255
        elif method == 4:
            for i in range(stride):
                left = row[i - channels] if i >= channels else 0
                above, upper_left = previous[i], previous[i - channels] if i >= channels else 0
                estimate = left + above - upper_left
                dl, da, du = abs(estimate - left), abs(estimate - above), abs(estimate - upper_left)
                predictor = left if dl <= da and dl <= du else above if da <= du else upper_left
                row[i] = (row[i] + predictor) & 255
        elif method != 0:
            raise ValueError(f"Unsupported PNG row filter {method}")
        rows.append(row)
        previous = row
    return width, height, channels, color_type, palette, rows


def groups(indices, counts):
    result = []
    for coordinate in indices:
        if result and coordinate == result[-1]["last"] + 1:
            result[-1]["last"] = coordinate
            result[-1]["pixel_counts"].append(counts[coordinate])
        else:
            result.append({"first": coordinate, "last": coordinate, "pixel_counts": [counts[coordinate]]})
    for group in result:
        group["thickness"] = group["last"] - group["first"] + 1
    return result


def measure(path, bounds, color, threshold):
    width, height, channels, color_type, palette, rows = png_rows(path)
    x0, y0, x1, y1 = bounds
    x0, y0, x1, y1 = max(x0, 0), max(y0, 0), min(x1, width - 1), min(y1, height - 1)
    column_counts, row_counts, exact_pixels = Counter(), Counter(), []
    target = bytes(color)
    for y in range(y0, y1 + 1):
        row = rows[y]
        if color_type in (2, 6):
            pos = row.find(target, x0 * channels, (x1 + 1) * channels)
            while pos >= 0:
                if pos % channels == 0:
                    x = pos // channels
                    row_counts[y] += 1
                    column_counts[x] += 1
                    exact_pixels.append((x, y))
                pos = row.find(target, pos + 1, (x1 + 1) * channels)
        else:
            for x in range(x0, x1 + 1):
                pixel = palette[row[x]] if color_type == 3 else (row[x * channels],) * 3
                if pixel == tuple(color):
                    row_counts[y] += 1
                    column_counts[x] += 1
                    exact_pixels.append((x, y))
    vertical = groups(sorted(x for x, count in column_counts.items() if count > threshold), column_counts)
    horizontal = groups(sorted(y for y, count in row_counts.items() if count > threshold), row_counts)
    for group in vertical:
        support = [y for x, y in exact_pixels if group["first"] <= x <= group["last"]]
        group["matching_support_inclusive"] = [min(support), max(support)]
    for group in horizontal:
        support = [x for x, y in exact_pixels if group["first"] <= y <= group["last"]]
        group["matching_support_inclusive"] = [min(support), max(support)]
    result = {
        "file": Path(path).name,
        "image_size": [width, height],
        "scan_bounds_inclusive": [x0, y0, x1, y1],
        "exact_rgb": list(color),
        "matching_pixels": len(exact_pixels),
        "minimum_pixel_count_exclusive": threshold,
        "vertical_strokes": vertical,
        "horizontal_strokes": horizontal,
        "rectangle": None,
        "detection_status": "no_qualifying_long_strokes" if not vertical and not horizontal else "partial_or_ambiguous_strokes",
        "partial_frame": None,
    }
    if len(vertical) == 2 and len(horizontal) == 2:
        result["detection_status"] = "complete_frame"
        left, right = vertical[0]["first"], vertical[1]["last"]
        top, bottom = horizontal[0]["first"], horizontal[1]["last"]
        result["rectangle"] = {
            "outer_bounds_inclusive": [left, top, right, bottom],
            "outer_pixel_width": right - left + 1,
            "outer_pixel_height": bottom - top + 1,
            "geometric_width": vertical[1]["first"] - vertical[0]["first"],
            "geometric_height": horizontal[1]["first"] - horizontal[0]["first"],
            "stroke_centers": {
                "left": (vertical[0]["first"] + vertical[0]["last"]) / 2,
                "right": (vertical[1]["first"] + vertical[1]["last"]) / 2,
                "top": (horizontal[0]["first"] + horizontal[0]["last"]) / 2,
                "bottom": (horizontal[1]["first"] + horizontal[1]["last"]) / 2,
            },
            "stroke_center_center": [
                sum(group["first"] + group["last"] for group in vertical) / 4,
                sum(group["first"] + group["last"] for group in horizontal) / 4,
            ],
            "all_strokes_2px": all(group["thickness"] == 2 for group in vertical + horizontal),
        }
    elif len(vertical) == 1 and len(horizontal) == 2 and all(group["matching_support_inclusive"][1] == x1 for group in horizontal):
        result["detection_status"] = "partial_frame_right_clipped"
        result["partial_frame"] = {
            "observed_left_stroke": [vertical[0]["first"], vertical[0]["last"]],
            "observed_top_stroke": [horizontal[0]["first"], horizontal[0]["last"]],
            "observed_bottom_stroke": [horizontal[1]["first"], horizontal[1]["last"]],
            "right_stroke": None,
            "right_stroke_note": "not observed; both horizontal strokes continue to the canvas/scan right boundary",
            "geometric_width": None,
            "geometric_height": horizontal[1]["first"] - horizontal[0]["first"],
            "outer_pixel_width": None,
            "outer_pixel_height": horizontal[1]["last"] - horizontal[0]["first"] + 1,
            "stroke_center_center": [None, sum(group["first"] + group["last"] for group in horizontal) / 4],
            "horizontal_strokes_reach_x": x1,
        }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("images", nargs="+", help="PNG paths or glob patterns")
    parser.add_argument("--bounds", type=int, nargs=4, default=[208, 364, 1959, 1619], metavar=("X0", "Y0", "X1", "Y1"))
    parser.add_argument("--rgb", type=int, nargs=3, default=[62, 133, 117])
    parser.add_argument("--threshold", type=int, default=400)
    parser.add_argument("--json", type=Path, help="Optional output path; otherwise JSON goes to stdout")
    parser.add_argument("--markdown", type=Path, help="Optional human-readable measurement report")
    parser.add_argument("--notes", type=Path, help="Optional JSON capture-state and visual-review notes to include in reports")
    args = parser.parse_args()
    paths = sorted(set(path for pattern in args.images for path in glob.glob(pattern)))
    results = []
    for path in paths:
        try:
            results.append(measure(path, args.bounds, args.rgb, args.threshold))
        except Exception as error:
            results.append({"file": Path(path).name, "error": str(error)})
    by_file = {result["file"]: result for result in results}
    comparisons = []
    baseline = by_file.get("A03-resized-rotated.png", {}).get("rectangle")
    if baseline:
        for filename in ("A06-back-to-3d.png", "A07-layout-original.png"):
            rectangle = by_file.get(filename, {}).get("rectangle")
            if rectangle:
                comparisons.append({
                    "baseline": "A03-resized-rotated.png",
                    "comparison": filename,
                    "geometric_width_delta": rectangle["geometric_width"] - baseline["geometric_width"],
                    "geometric_height_delta": rectangle["geometric_height"] - baseline["geometric_height"],
                    "stroke_center_delta": [a - b for a, b in zip(rectangle["stroke_center_center"], baseline["stroke_center_center"])],
                    "all_frame_geometry_identical": rectangle == baseline,
                })
    document = {
        "coordinate_system": "zero-based raster pixel indices; stroke ranges, outer bounds and scan bounds are inclusive",
        "method": {
            "exact_rgb": args.rgb,
            "scan_bounds_inclusive": args.bounds,
            "line_detection": f"group consecutive rows/columns with strictly more than {args.threshold} exact-color pixels",
            "geometric_dimensions": "right.first-left.first and bottom.first-top.first; distances between corresponding stroke edges",
            "outer_pixel_dimensions": "outermost stroke pixel minus innermost opposite outer pixel plus one; includes both stroke thicknesses",
            "center": "mean of left/right stroke midpoints and mean of top/bottom stroke midpoints",
            "handles": "compact handles cannot add qualifying long rows/columns and therefore do not enlarge the frame measurement",
            "limitations": "reports frame geometry only; no inference about molecule orientation, model contents, or other screenshot state",
        },
        "measurements": results,
        "comparisons": comparisons,
    }
    notes = json.loads(args.notes.read_text()) if args.notes else {}
    if notes:
        document["review_notes"] = notes
    output = json.dumps(document, indent=2) + "\n"
    if args.json:
        args.json.write_text(output)
    else:
        print(output, end="")
    if args.markdown:
        report = [
            "# PNG frame measurements",
            "",
            "Measurements use exact PNG RGB bytes `(62, 133, 117)` in the inclusive canvas region `x208..1959, y364..1619`. Coordinates below are zero-based screenshot pixel indices. Consecutive rows or columns containing **more than 400** exact-color pixels form a stroke. The compact handles do not create qualifying long rows or columns and cannot enlarge the measured frame.",
            "",
            "**Geometric dimensions** are `right.first − left.first` and `bottom.first − top.first`, the distance between corresponding edges of the opposite strokes. **Outer footprint** includes every pixel of both strokes, so a frame with 2px strokes has a footprint two pixels larger than its geometric dimensions. The center is the mean of opposite stroke midpoints. To convert raster-index centers to physical point coordinates for a 2px/pt screenshot, add 0.5px to each coordinate and divide by 2.",
            "",
            "| Screenshot | Vertical stroke columns | Horizontal stroke rows | Geometric size (px) | Outer footprint (px) | Stroke-center center (px) |",
            "|---|---|---|---:|---:|---:|",
        ]
        for result in results:
            rectangle = result.get("rectangle")
            if rectangle:
                vs = "; ".join(f"{group['first']}..{group['last']}" for group in result["vertical_strokes"])
                hs = "; ".join(f"{group['first']}..{group['last']}" for group in result["horizontal_strokes"])
                center = ", ".join(str(value) for value in rectangle["stroke_center_center"])
                report.append(f"| `{result['file']}` | {vs} | {hs} | {rectangle['geometric_width']} × {rectangle['geometric_height']} | {rectangle['outer_pixel_width']} × {rectangle['outer_pixel_height']} | ({center}) |")
            elif result.get("partial_frame"):
                partial = result["partial_frame"]
                vs = "; ".join(f"{group['first']}..{group['last']}" for group in result["vertical_strokes"])
                hs = "; ".join(f"{group['first']}..{group['last']}" for group in result["horizontal_strokes"])
                report.append(f"| `{result['file']}` | {vs}; right clipped | {hs} | unknown × {partial['geometric_height']} | unknown × {partial['outer_pixel_height']} | (unknown, {partial['stroke_center_center'][1]}) |")
            else:
                report.append(f"| `{result['file']}` | No qualifying >400px stroke | No qualifying >400px stroke | — | — | — |")
        unequal_strokes = [result["file"] for result in results if result.get("rectangle") and not result["rectangle"]["all_strokes_2px"]]
        if unequal_strokes:
            report.extend(["", "These captures have one or more exact-color strokes whose thickness differs from 2px: " + ", ".join(f"`{filename}`" for filename in unequal_strokes) + ". Their dimensions use the same corresponding-first-edge formula shown above; the JSON preserves every observed stroke thickness. No antialiased pixels or inferred stroke edges are added."])
        clipped = [result["file"] for result in results if result.get("partial_frame")]
        if clipped:
            report.extend(["", "The selected Both captures " + ", ".join(f"`{filename}`" for filename in clipped) + " have a visible left edge and two horizontal strokes reaching `x1959`, the canvas/scan right boundary. Their right edges are clipped by the properties sidebar. Only observed strokes and the height are reported. Full width, horizontal center, and missing right-edge position are **unknown** and are not inferred."])
        report.extend(["", "## A03 → A06 → A07 comparison", ""])
        if comparisons:
            for comparison in comparisons:
                report.append(f"- `{comparison['comparison']}` versus `{comparison['baseline']}`: geometric width Δ{comparison['geometric_width_delta']}px, height Δ{comparison['geometric_height_delta']}px, center Δ{tuple(comparison['stroke_center_delta'])}px. All recorded frame geometry identical: **{comparison['all_frame_geometry_identical']}**.")
        else:
            report.append("Required screenshots have not all been measured yet.")
        report.extend([
            "",
            "The first A02 attempt is retained as setup evidence: the capture owner reported that keyboard drawing was initially enabled and that the first drag did not resize the frame. The subsequent `A02-resized.png` is the successful resize capture.",
            "",
            "The numeric measurements compare frame geometry only. They do not establish whether the molecule orientation or other screenshot content is identical. 'No qualifying >400px stroke' does not mean no visible selection frame: a smaller visible frame can fall below this threshold, as in A05. Such captures are not assigned inferred dimensions.",
        ])
        if notes:
            report.extend(["", "## Capture-state notes", ""])
            for note in notes.get("capture_state_notes", []):
                report.append(f"- {note}")
            report.extend(["", "## Independent visual review", ""])
            for note in notes.get("visual_review", []):
                report.append(f"- {note}")
        report.extend([
            "",
            "## Reproduce",
            "",
            "```sh",
            "python3 tools/measure_png_frames.py 'A*.png' --json frame-measurements.json --markdown frame-measurements.md" + (" --notes tools/frame_review_notes.json" if args.notes else ""),
            "```",
            "",
            "The tool uses only the Python standard library and reads stored PNG color bytes without display color conversion. The JSON includes image dimensions, exact match counts, each qualifying stroke's full interval and per-coordinate match counts, inclusive outer bounds, geometric dimensions, outer footprint, and center.",
            "",
        ])
        args.markdown.write_text("\n".join(report))


if __name__ == "__main__":
    main()
