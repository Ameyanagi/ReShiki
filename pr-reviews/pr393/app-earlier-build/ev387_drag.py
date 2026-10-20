"""Operator-authorized guarded Quartz held-button/Option gestures."""
import argparse
import json
import time
import Quartz
from ev387_guard import ARTIFACTS, capture, verify

parser = argparse.ArgumentParser()
parser.add_argument("--start", required=True)
parser.add_argument("--via", action="append", required=True,
                    help="x,y or x,y,capture.png; each segment takes 1 second")
parser.add_argument("--option", action="store_true")
parser.add_argument("--resume-held", action="store_true")
parser.add_argument("--released")
parser.add_argument("--log", required=True)
args = parser.parse_args()
start = tuple(map(float, args.start.split(",")))
waypoints = []
for value in args.via:
    parts = value.split(",")
    waypoints.append((tuple(map(float, parts[:2])), parts[2] if len(parts) > 2 else None))
source = Quartz.CGEventSourceCreate(Quartz.kCGEventSourceStateHIDSystemState)
flags = Quartz.kCGEventFlagMaskAlternate if args.option else 0
log = {"start": start, "option": args.option, "checkpoints": []}
last = start


def post(kind, point):
    state = verify([point])
    event = Quartz.CGEventCreateMouseEvent(source, kind, point, Quartz.kCGMouseButtonLeft)
    Quartz.CGEventSetFlags(event, flags)
    Quartz.CGEventSetIntegerValueField(event, Quartz.kCGMouseEventClickState, 1)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, event)
    return state


log["resumed_held_button"] = args.resume_held
log["initial"] = verify([start]) if args.resume_held else post(Quartz.kCGEventLeftMouseDown, start)
time.sleep(0.15)
try:
    for destination, filename in waypoints:
        origin = last
        for index in range(1, 31):
            ratio = index / 30
            last = tuple(origin[k] + (destination[k] - origin[k]) * ratio for k in range(2))
            post(Quartz.kCGEventLeftMouseDragged, last)
            time.sleep(1 / 30)
        time.sleep(0.35)
        log["checkpoints"].append({
            "point": last,
            "state": capture(filename) if filename else verify([last]),
            "button_held": True,
        })
finally:
    # Verify the target again before the release event as for all other inputs.
    log["release_state"] = post(Quartz.kCGEventLeftMouseUp, last)
    time.sleep(0.4)
    if args.released:
        log["released_capture"] = capture(args.released)
    (ARTIFACTS / args.log).write_text(json.dumps(log, indent=2) + "\n")
print(json.dumps(log, indent=2))
