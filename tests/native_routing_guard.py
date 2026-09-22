"""Record every Python startup; migrated default routes must never reach here."""

import json
import sys
from pathlib import Path

record = Path(__file__).resolve().parents[1] / "python-started"
with record.open("a", encoding="utf-8") as output:
    output.write("started\n")

for line in sys.stdin:
    request = json.loads(line)
    print(
        json.dumps(dict(id=request.get("id"), ok=False, error="Python routing guard")),
        flush=True,
    )
