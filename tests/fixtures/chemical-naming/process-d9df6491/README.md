# Supplemental current-source naming evidence

This package records production source
`d9df649129c5144cf304df7295dc573825dcbe49` and the unique signed
**ReShiki Local Naming Process Verified.app**, executable SHA256
`062b03f1d966e27301272a44cdbff243f002c2f59da1531dcd5d2f01f2b9dda5`.
See the [review and reproduction steps](../../../../docs/changes/local-chemical-naming.md).
The earlier [aaa6/1c7d fixture package](../local/README.md) and all its original
bytes remain separate and unchanged.

| Original native file | Saved result |
| --- | --- |
| `ethanol-inserted.rsk` | Complete neutral ethanol, three atoms/two bonds, no annotations. |
| `ethanol-undo-empty.rsk` | One header Undo removes the complete graph. |
| `ethanol-redo.rsk` | One Redo restores every original native byte. |
| `ethanol-fresh-reopened.rsk` | Fresh process Open/Save As preserves the same bytes; Undo/Redo start disabled. |

All native files are version 19. The three nonempty files have identical SHA256
`7c5087f6e96abaedc71261ce0e174d089ddaa78648a500c36897ad6695bb899c`.
Three [original JPEGs](../../../../docs/images/chemical-naming/process-d9df6491/ethanol-local-preview.jpg)
and five original accessibility snapshots accompany the native saves. The
[desktop receipt](native-desktop-receipt.json) records exact actions, process
observation limits, capture settings and scopes not repeated.

[provenance.json](provenance.json) hashes all 25 payload files, including copied
raw receipts/logs and the derived compact [CI summary](ci-native-summary.json).
The latter retains source receipt hashes, all check/job URLs, native log hashes
and prior failure details without copying the large CI logs. No CI query or
rerun was performed while assembling this package.

The [bundle metadata](bundle-final-metadata.json) was frozen before the final
desktop smoke, so its pending-desktop field is historical; the later original
desktop receipt records the completed check. Likewise, the old failure and
compiler-count receipts retain their original wording. Do not read those
earlier pending/error fields as a new application failure or silently relabel
them. Current production d9 CI passed 22 checks with three intentional skips;
an evidence-only follow-up commit has its own CI status.

The [compiler receipt](own-compiler-fresh-proof.json) covers the selected final
Mac app graph: 17 own artifacts, 14 packages, all `fresh:false`, and 574 hashed
absolute own-source inputs. The [license receipt](opsin-license-byte-proof.json)
confirms all 39 original upstream records are included verbatim. The
[admission receipt](admission-validation-summary.json) records a controlled old
failure, 20 corrected executor passes and all six unchanged runtime integration
passes. Local tests in the three `.log` files ran with explicit network denial;
the desktop process did not have a denial wrapper.

Run the following from the repository root. It uses only Python's standard
library and the existing maintainer evidence helper; Python is not an app
runtime dependency. It verifies original bytes, native history, independent
ethanol topology/hydrogens, current-source compiler input bytes and receipt
scope. It does not replay the GUI, rerun naming or rebuild the signed app.

```sh
python3 scripts/verify_chemical_naming_desktop.py
python3 - <<'PY'
import hashlib
import json
import runpy
from pathlib import Path

root = Path.cwd()
base = root / "tests/fixtures/chemical-naming/process-d9df6491"
manifest = json.loads((base / "provenance.json").read_bytes())
assert len(manifest["files"]) == 25
for entry in manifest["files"]:
    data = (root / entry["path"]).read_bytes()
    assert len(data) == entry["bytes"]
    assert hashlib.sha256(data).hexdigest() == entry["sha256"]
    if entry["path"].endswith(".jpg"):
        assert data.startswith(b"\xff\xd8\xff")
names = ["ethanol-inserted.rsk", "ethanol-redo.rsk", "ethanol-fresh-reopened.rsk"]
raw = [(base / name).read_bytes() for name in names]
assert raw[0] == raw[1] == raw[2]
check = runpy.run_path("scripts/verify_chemical_naming_desktop.py")["check_molecule"]
for data in raw:
    doc = json.loads(data)
    assert doc["version"] == 19 and check(doc, False) == "C2H6O"
    assert not any(doc[key] for key in ["annotations", "arrows", "graphics", "groups"])
undo = json.loads((base / "ethanol-undo-empty.rsk").read_bytes())
assert undo["version"] == 19
assert not any(undo[key] for key in ["atoms", "bonds", "annotations", "arrows", "graphics", "groups"])
proof = json.loads((base / "own-compiler-fresh-proof.json").read_bytes())
assert proof["own_artifact_count"] == 17 and proof["own_package_count"] == 14
assert proof["all_own_artifacts_fresh_false"] and len(proof["inputs"]) == 574
for item in proof["inputs"]:
    assert hashlib.sha256((root / item["worktree_relative_path"]).read_bytes()).hexdigest() == item["sha256"]
ci = json.loads((base / "ci-native-summary.json").read_bytes())
assert ci["source_commit"] == manifest["source_commit"] and ci["terminal"]
assert ci["totals"] == {"SUCCESS": 22, "SKIPPED": 3}
assert len(ci["required_naming_hosts"]) == 3 and len(ci["native_process_hosts"]) == 6
print("PASS: 25 original/derived hashes, ethanol history/identity, 574 compiler inputs, exact-source CI scope")
PY
```

[byte-verification.json](byte-verification.json) records this check and separate
semantic mutation canaries. The manifest excludes itself, this README and that
derived verification output to avoid self-referential hashes.

All copied screenshots, native saves, accessibility snapshots and raw receipts
retain their original bytes. Original controlled evidence/documentation is
MIT OR Apache-2.0; bundled upstream records retain their existing separate terms.
