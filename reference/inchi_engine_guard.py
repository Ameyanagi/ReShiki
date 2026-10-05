"""Real worker with native key generation forbidden and InChI calls counted."""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from rdkit import Chem
from rdkit.Chem import inchi, rdinchi

from engine import worker


def forbidden(*args, **kwargs):
    raise AssertionError("Migrated engine called native InChIKey generation")


original = worker.handle
native_inchi = Chem.MolToInchi
record = Path(sys.argv[1])
fault = sys.argv[2] if len(sys.argv) > 2 else None
inchi_calls = 0


def counted_inchi(*args, **kwargs):
    global inchi_calls
    inchi_calls += 1
    return native_inchi(*args, **kwargs)


def guarded(request):
    global inchi_calls
    inchi_calls = 0
    result = original(request)
    analysis = result.get("analysis")
    if analysis is not None:
        if request.get("local_properties") is not True or "inchikey" in analysis:
            raise AssertionError("Analysis did not request a Rust InChIKey")
        if fault == "native-key":
            analysis["inchikey"] = "VNWKTOKETHGBQD-UHFFFAOYSA-N"
        elif fault == "missing-inchi":
            del analysis["inchi"]
        elif fault == "invalid-inchi":
            analysis["inchi"] = "invalid"
    with record.open("a", encoding="utf-8") as output:
        output.write(
            json.dumps(
                dict(
                    operation=request.get("operation"),
                    format=request.get("format"),
                    analysis=analysis is not None,
                    inchi_calls=inchi_calls,
                )
            )
            + "\n"
        )
    return result


Chem.MolToInchi = counted_inchi
Chem.MolToInchiKey = forbidden
inchi.MolToInchiKey = forbidden
inchi.InchiToInchiKey = forbidden
rdinchi.MolToInchiKey = forbidden
rdinchi.InchiToInchiKey = forbidden
worker.handle = guarded
worker.main()
