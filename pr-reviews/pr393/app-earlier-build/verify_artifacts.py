"""Read-only evidence and process verification; does not interact with any UI."""
from pathlib import Path
import re
import struct
import subprocess

root = Path(__file__).resolve().parent
required = [
    "01-c-o.png", "02-marks-added.png", "03-lonepair-mid-drag.png",
    "03b-lonepair-mid-drag-2.png", "04-lonepair-released.png",
    "05-option-mid-drag.png", "05b-option-released.png",
    "06-charge-mid-drag.png", "06b-charge-released.png",
    "07-rotated.png", "08-undo-rotate.png",
]
for name in required:
    data = (root / name).read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", name
    width, height = struct.unpack(">II", data[16:24])
    assert (width, height) == (2560, 1704), (name, width, height)
    print(f"{name}: {width}x{height}, {len(data)} bytes")
report = (root / "report.md").read_text()
for link in re.findall(r"\]\(([^)]+)\)", report):
    assert (root / link).exists(), f"Missing linked artifact: {link}"
for pid in (75972, 81119):
    result = subprocess.run(["ps", "-p", str(pid), "-o", "pid="], capture_output=True)
    assert result.returncode != 0, f"Branch PID {pid} still running"
result = subprocess.run(["ps", "-p", "51217", "-o", "comm="], capture_output=True, text=True)
assert result.returncode == 0 and "/nix/store/" in result.stdout
print("All required images and report links verified; branch PIDs exited; installed ReShiki remains running.")
