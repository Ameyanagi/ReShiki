"""Real worker transport with native CIP forbidden; used by Rust engine tests."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from rdkit.Chem import rdCIPLabeler

from engine import prepared, worker


def forbidden(*args, **kwargs):
    raise AssertionError("Migrated engine called native CIP labeling")


original = worker.handle
record = Path(sys.argv[1])
inject = len(sys.argv) > 2 and sys.argv[2] == "inject-labels"


def guarded(request):
    if request.get("operation") == "label_reaction":
        forbidden()
    if request.get("prepared_drawing") is not None and request.get("local_cip") is not True:
        raise AssertionError("Prepared drawing did not request Rust CIP")
    with record.open("a", encoding="utf-8") as output:
        output.write(
            json.dumps(
                {
                    k: request.get(k)
                    for k in ("operation", "format", "local_cip", "prepared_reaction")
                }
            )
            + "\n"
        )
    result = original(request)
    if inject:
        result["drawing_labels"] = {}
    return result


prepared.label_drawing = forbidden
rdCIPLabeler.AssignCIPLabels = forbidden
worker.handle = guarded
worker.main()
