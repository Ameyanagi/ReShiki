"""A stdlib-only child for testing the Rust reference transport without chemistry."""

import json
import sys
from pathlib import Path


def main():
    for line in sys.stdin:
        request = json.loads(line)
        action = request["action"]
        if action == "eof":
            return
        if action == "stall":
            Path(request["ready"]).write_text(str(request["id"]), encoding="utf-8")
            continue
        if action == "malformed":
            print("{", flush=True)
            continue
        if action == "reply":
            response = request["response"]
            if isinstance(response, dict) and request.get("attach_id", True):
                response = {"id": request["id"], **response}
        else:
            response = {"id": request["id"], "ok": True, "result": request["payload"]}
        print(json.dumps(response, allow_nan=False), flush=True)


if __name__ == "__main__":
    main()
