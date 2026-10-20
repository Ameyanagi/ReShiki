"""Read-only window identity/coverage checks and isolated window captures."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import Quartz

ARTIFACTS = Path(__file__).resolve().parent
BUNDLE_ID = "dev.reshiki.ev387branch"


def verify(points=()):
    front = subprocess.check_output(["lsappinfo", "front"], text=True).strip()
    info = subprocess.check_output(
        ["lsappinfo", "info", "-only", "bundleid,pid,name", front], text=True
    )
    if f'bundleID="{BUNDLE_ID}"' not in info:
        raise RuntimeError(f"Refusing input: branch app is not frontmost: {info}")
    pid = int(re.search(r"pid = (\d+)", info).group(1))
    windows = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly | Quartz.kCGWindowListExcludeDesktopElements,
        Quartz.kCGNullWindowID,
    )
    own = [
        w for w in windows
        if w.get("kCGWindowOwnerPID") == pid and w.get("kCGWindowLayer") == 0
        and w["kCGWindowBounds"]["Width"] > 300
    ]
    if len(own) != 1:
        raise RuntimeError(f"Expected one branch document window, got {len(own)}")
    window = own[0]
    for x, y in points:
        covering = []
        for w in windows:
            if (w.get("kCGWindowOwnerName") == "ChatGPT Computer Use"
                and w.get("kCGWindowName") == "Software Cursor"):
                continue  # Non-interactive pointer visualization, not an app window.
            if (w.get("kCGWindowOwnerName") == "Window Server"
                and w.get("kCGWindowName") == "Cursor"):
                continue  # OS pointer sprite, not an app window.
            b = w["kCGWindowBounds"]
            if (w.get("kCGWindowAlpha", 1) > 0
                and b["X"] <= x < b["X"] + b["Width"]
                and b["Y"] <= y < b["Y"] + b["Height"]):
                covering.append(w)
        if not covering or covering[0].get("kCGWindowOwnerPID") != pid:
            raise RuntimeError(f"Refusing input: point {(x, y)} is not over the branch window")
    return {
        "bundle_id": BUNDLE_ID,
        "pid": pid,
        "window_id": window["kCGWindowNumber"],
        "bounds": dict(window["kCGWindowBounds"]),
        "title": window.get("kCGWindowName"),
        "checked_points": list(points),
    }


def capture(filename):
    state = verify()
    path = ARTIFACTS / filename
    if path.parent != ARTIFACTS or path.suffix != ".png":
        raise ValueError("Capture must be a PNG directly in the artifact directory")
    subprocess.run(
        ["screencapture", "-x", "-o", "-l", str(state["window_id"]), str(path)],
        check=True,
    )
    return {**state, "capture": str(path)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--point", action="append", default=[])
    parser.add_argument("--capture")
    args = parser.parse_args()
    points = [tuple(map(float, p.split(","))) for p in args.point]
    state = verify(points)
    if args.capture:
        state = capture(args.capture)
    print(json.dumps(state, indent=2))
