#!/usr/bin/env python3
"""Run 2 measurements; never modifies Run 1 artifacts.

Usage: python3 -B measurement-frames-run2.py Q0[1-5]-*.png > measurements-pixels-run2.json
PNG decoding is inherited read-only from measurement-frames.py. Coordinates are
0-based original screenshot pixels; every rectangle endpoint is inclusive.
"""
import collections
import hashlib
import json
import pathlib
import runpy
import sys

BASE = runpy.run_path(str(pathlib.Path(__file__).with_name("measurement-frames.py")))
read_png, runs = BASE["read_png"], BASE["runs"]
TARGET = (62, 133, 117)
BAR = (214, 219, 223)
WHITE = (255, 255, 255)
CANVAS = (208, 364, 1959, 1619)
MIN_RUN = 100


def long_runs(by_axis, minimum=MIN_RUN):
    return [[axis, a, b, b-a+1]
            for axis, values in sorted(by_axis.items())
            for a, b in runs(values) if b-a+1 >= minimum]


def envelope(points):
    xs, ys = zip(*points)
    return [min(xs), min(ys), max(xs), max(ys)]


def size(rect):
    return [rect[2]-rect[0]+1, rect[3]-rect[1]+1]


def analyze(path):
    w, h, rows, bpp, pixel = read_png(path)
    x0, y0, x1, y1 = CANVAS
    assert w > x1 and h > y1, "Canvas does not fit screenshot"
    frame = set()
    frame_by_x, frame_by_y = collections.defaultdict(list), collections.defaultdict(list)
    bar_by_y = collections.defaultdict(list)
    frame_rgb, bar_rgb = bytes(TARGET), bytes(BAR)
    for y in range(y0, y1+1):
        row = rows[y]
        for x in range(x0, x1+1):
            rgb = row[x*bpp:x*bpp+3]
            if rgb == frame_rgb:
                frame.add((x, y))
                frame_by_x[x].append(y)
                frame_by_y[y].append(x)
            elif rgb == bar_rgb:
                bar_by_y[y].append(x)
    horizontal, vertical = long_runs(frame_by_y), long_runs(frame_by_x)
    straight = set()
    for y, a, b, n in horizontal:
        straight.update((x, y) for x in range(a, b+1))
    for x, a, b, n in vertical:
        straight.update((x, y) for y in range(a, b+1))
    mask = bytes(int((x, y) in frame) for y in range(y0, y1+1) for x in range(x0, x1+1))
    out = {
        "file": str(path), "size": [w, h], "source_sha256": hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest(),
        "target": TARGET, "canvas_inclusive": CANVAS,
        "exact_color_canvas_count": len(frame),
        "horizontal_long_runs_y_x0_x1_len": horizontal,
        "vertical_long_runs_x_y0_y1_len": vertical,
        "exact_color_long_straight_run_count": len(straight),
        "exact_color_canvas_mask_sha256": hashlib.sha256(mask).hexdigest(),
    }
    frame_rect = None
    if horizontal and vertical:
        vx = sorted({r[0] for r in vertical})
        hy = sorted({r[0] for r in horizontal})
        frame_rect = [min(vx), min(hy), max(vx), max(hy)]
        out["straight_edge_exact_color_envelope_inclusive"] = frame_rect
        out["straight_edge_exact_color_envelope_size"] = size(frame_rect)
        vgroups, hgroups = runs(vx), runs(hy)
        out["vertical_edge_core_column_groups_inclusive"] = vgroups
        out["horizontal_edge_core_row_groups_inclusive"] = hgroups
        if len(vgroups) == 2 and len(hgroups) == 2:
            mx = [(a+b)/2 for a, b in vgroups]
            my = [(a+b)/2 for a, b in hgroups]
            out["exact_color_core_centerline_distances"] = [mx[1]-mx[0], my[1]-my[0]]
            out["four_edge_cores"] = {
                "top": [r for r in horizontal if hgroups[0][0] <= r[0] <= hgroups[0][1]],
                "bottom": [r for r in horizontal if hgroups[1][0] <= r[0] <= hgroups[1][1]],
                "left": [r for r in vertical if vgroups[0][0] <= r[0] <= vgroups[0][1]],
                "right": [r for r in vertical if vgroups[1][0] <= r[0] <= vgroups[1][1]],
            }

    # Detect the floating rounded bar anywhere in the canvas. Its two long gray
    # horizontal border cores overlap horizontally and are 48..100 px apart.
    # This avoids the Run 1 assumption that the bar must be below the frame.
    gray_runs = long_runs(bar_by_y)
    out["floating_bar_candidate_horizontal_border_runs"] = gray_runs
    candidates = []
    for upper in gray_runs:
        for lower in gray_runs:
            if 48 <= lower[0]-upper[0] <= 100:
                overlap = min(upper[2], lower[2])-max(upper[1], lower[1])+1
                if overlap >= 300 and abs(upper[1]-lower[1]) <= 8 and abs(upper[2]-lower[2]) <= 8:
                    candidates.append((overlap, upper, lower))
    assert candidates, "No complete floating bar border detected"
    _, upper, lower = max(candidates)
    bx0, bx1 = min(upper[1], lower[1])-32, max(upper[2], lower[2])+32
    by0, by1 = upper[0]-4, lower[0]+4
    border = [(x, y) for y in range(max(y0, by0), min(y1, by1)+1)
              for x in range(max(x0, bx0), min(x1, bx1)+1) if pixel(x, y) == BAR]
    bar_rect = envelope(border)
    out["floating_bar_exact_border_rgb"] = BAR
    out["floating_bar_exact_border_pixel_count"] = len(border)
    out["floating_bar_exact_border_envelope_inclusive"] = bar_rect
    out["floating_bar_exact_border_envelope_size"] = size(bar_rect)
    out["floating_bar_exact_border_canvas_clearance_left_top_right_bottom"] = [
        bar_rect[0]-x0, bar_rect[1]-y0, x1-bar_rect[2], y1-bar_rect[3]]
    if frame_rect:
        if bar_rect[3] < frame_rect[1]:
            placement = "above"
            gap = frame_rect[1]-bar_rect[3]-1
        elif bar_rect[1] > frame_rect[3]:
            placement = "below"
            gap = bar_rect[1]-frame_rect[3]-1
        else:
            placement, gap = "vertical overlap", None
        out["floating_bar_placement_relative_to_frame"] = placement
        out["floating_bar_pixel_row_separation_between_border_and_frame_core"] = gap
        out["floating_bar_center_x_minus_frame_center_x"] = (bar_rect[0]+bar_rect[2]-frame_rect[0]-frame_rect[2])/2
        out["floating_bar_horizontal_overlap_with_frame_in_pixels"] = max(0, min(bar_rect[2], frame_rect[2])-max(bar_rect[0], frame_rect[0])+1)

    # Nonwhite bounds in a local region enclose paint and shadow. Record ROI
    # contact explicitly, rather than assuming unbounded shadow segmentation.
    shadow_roi = [max(x0, bar_rect[0]-24), max(y0, bar_rect[1]-4), min(x1, bar_rect[2]+24), min(y1, bar_rect[3]+24)]
    def is_bar_paint_or_shadow(x, y):
        r, g, b = pixel(x, y)
        # Green-dominant pixels include the adjacent selection rotation handle.
        # Removing that hue cannot move the bar's gray border/shadow envelope.
        return (r, g, b) != WHITE and not (g > r and g > b)
    painted = [(x, y) for y in range(shadow_roi[1], shadow_roi[3]+1)
               for x in range(shadow_roi[0], shadow_roi[2]+1) if is_bar_paint_or_shadow(x, y)]
    painted_rect = envelope(painted)
    out["floating_bar_shadow_scan_roi_inclusive"] = shadow_roi
    out["floating_bar_paint_and_shadow_envelope_inclusive"] = painted_rect
    out["floating_bar_paint_touches_shadow_roi_left_top_right_bottom"] = [
        painted_rect[0] == shadow_roi[0], painted_rect[1] == shadow_roi[1],
        painted_rect[2] == shadow_roi[2], painted_rect[3] == shadow_roi[3]]
    out["floating_bar_including_shadow_canvas_clearance_left_top_right_bottom"] = [
        painted_rect[0]-x0, painted_rect[1]-y0, x1-painted_rect[2], y1-painted_rect[3]]
    if frame_rect:
        if placement == "above":
            out["floating_bar_including_shadow_pixel_row_separation_to_frame_core"] = frame_rect[1]-painted_rect[3]-1
        elif placement == "below":
            out["floating_bar_including_shadow_pixel_row_separation_to_frame_core"] = painted_rect[1]-frame_rect[3]-1
    return out, frame, straight


def main(paths):
    measurements, masks, straights = [], {}, {}
    for path in paths:
        out, mask, straight = analyze(path)
        key = pathlib.Path(path).name[:3]
        assert key not in masks, "Pass one screenshot per Q identifier"
        measurements.append(out)
        masks[key], straights[key] = mask, straight
    by_id = {pathlib.Path(m["file"]).name[:3]: m for m in measurements}
    comparisons = {}
    for left, right in [("Q03", "Q04"), ("Q01", "Q05")]:
        if left in masks and right in masks:
            comparisons[left+"_versus_"+right] = {
                "exact_color_canvas_mask_symmetric_difference_pixels": len(masks[left] ^ masks[right]),
                "long_straight_run_mask_symmetric_difference_pixels": len(straights[left] ^ straights[right]),
                "exact_color_canvas_masks_identical": masks[left] == masks[right],
            }
    if "Q01" in by_id and "Q03" in by_id:
        a, b = by_id["Q01"], by_id["Q03"]
        aw, ah = a["straight_edge_exact_color_envelope_size"]
        bw, bh = b["straight_edge_exact_color_envelope_size"]
        comparisons["Q03_divided_by_Q01"] = {
            "envelope_width_ratio": bw/aw, "envelope_height_ratio": bh/ah,
            "envelope_area_ratio": (bw*bh)/(aw*ah),
            "both_envelope_linear_dimensions_under_2x": bw < 2*aw and bh < 2*ah,
            "exact_color_count_ratio": b["exact_color_canvas_count"]/a["exact_color_canvas_count"],
            "long_straight_run_count_ratio": b["exact_color_long_straight_run_count"]/a["exact_color_long_straight_run_count"],
        }
        ac, bc = a.get("exact_color_core_centerline_distances"), b.get("exact_color_core_centerline_distances")
        if ac and bc:
            comparisons["Q03_divided_by_Q01"]["core_centerline_distance_ratios"] = [bc[i]/ac[i] for i in (0, 1)]
    return {
        "conventions": {
            "coordinates": "0-based original image pixels; all rectangle endpoints inclusive",
            "canvas": CANVAS,
            "frame_extents": "exact RGB(62,133,117) straight-run core; excludes antialias and short handle/stem runs",
            "frame_mask": "all exact RGB(62,133,117) pixels inside canvas; includes exact-color handle/stem pixels",
            "bar_extents": "exact RGB(214,219,223) border core; excludes antialias and shadow",
            "centerline": "midpoints of exact-color row/column groups, not geometric subpixel line estimates",
            "long_straight_run_count": "union of all horizontal/vertical exact-color contiguous runs of length at least 100; no double count",
            "gap": "pixel row separation between inclusive exact-color edges; intervening rotation handle/stem is excluded from the frame-edge definition, so rows are not necessarily all blank",
            "paint_envelope": "nonwhite pixels in stated local ROI around bar, excluding green-dominant pixels (G>R and G>B) of the adjacent selection rotation handle; ROI contact reported as possible contamination/truncation",
        },
        "measurements": measurements, "comparisons": comparisons,
    }


if __name__ == "__main__":
    print(json.dumps(main(sys.argv[1:]), indent=2))
