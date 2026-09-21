"""Original editable-drawing writer, independent of the Rust replacement."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from engine import worker  # noqa: E402


def main():
    for line in sys.stdin:
        try:
            request = json.loads(line)
            if request.get("operation") == "import":
                result = worker.handle(request)
            else:
                result = dict(
                    output=worker.export_cdxml(
                        request["document"],
                        request.get("text_layout"),
                        request.get("graphic_paths"),
                        request.get("graphic_parts"),
                        request.get("atom_indicators"),
                        request.get("picture_exports"),
                    )
                )
            print(json.dumps(result, allow_nan=False), flush=True)
        except (ValueError, RuntimeError, KeyError, OverflowError) as error:
            print(json.dumps(dict(error=str(error))), flush=True)


if __name__ == "__main__":
    main()
