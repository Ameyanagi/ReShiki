# Combined drawing evidence

The **authored reference** and the **actual native saves** have distinct provenance. The authored v24 drawing combines four pinned historical inputs into 68 atoms / 96 bonds, one linked cubic arrow, one PNG/EMF picture and the C60's 25% scope. It is an in-memory arrangement, not a GUI save. The original input versions 22, 17, 21 and 20 and their bytes remain intact.

ROOT later opened this drawing in the qualified default app at source `29157b3c66c70d627ea3828f305faa0d51a65e9f`, tree `2727374049423cd44cb1a28d716cbadd0cba45ec`. The packet contains **19 untouched JPEGs, 19 AX records and three native saves** from that review. Captures 17–19 show the original C60 in a separate tab at 209% in the same app; the combined-document views use 53%, 61%, 161% and 252%. These are not matched before/after screenshots of different app builds. AX records 01 and 02 are 113-byte early states, not complete accessibility trees.

Run with Python **3.9 or newer**, only the standard library, without `-O`. From this packet directory:

```sh
python3 verify_evidence.py
python3 verify_evidence.py --manifest manifest.json
```

The script may also be invoked by absolute path from another directory; its evidence reads stay inside the packet. The first command audits only the authored reference. The second hashes the frozen actual files, checks the restored native against the originals, verifies the fixed-carbon 18pt Stretch result and requires the fresh-process reopened save to be byte-identical to restored. It also compares the declared source, tree, executable and bundle identity across the authored correlation summaries. ROOT's publication review is separate from these static commands.

## What is checked

- The v24 reference has 68 atoms / 96 bonds, disjoint remapped chemistry, retained depth/style and unique object IDs. C60 has 60 atoms / 90 bonds and its 25% scope/weights. The cubic arrow retains both controls, lone-pair/atom targets and local mark ID 1. The exact original PNG and EMF payloads remain paired. No NMR report, plot, labels, palette or camera state is stored.
- Restored native JSON must match the whole authored document at binary32 precision, with only enumerated ordinary defaults from the minimal v17 input and legacy bond-ID removal. The arrow's load reconciliation is reconstructed from the original owner positions, mark offset, rigid translation and anchors. Both endpoints, adjacent controls and canonical offsets must match that result exactly. Targets, direction, gap and styles remain unchanged. No general tolerance was widened.
- For this pinned arrow, loading moves the departure endpoint and adjacent control by one binary32 ULP (`-0.00006103515625` world units in y); the arrival endpoint/control do not move. The original exact-offset checker failure is retained in `records/original-native-check.stderr.txt`. The narrow successor follows the reviewed source arithmetic; it is not a general reconciliation or clearance engine.
- Stretch is independently reconstructed from the original 14.4pt axis and the source-defined 18pt command. Only oxygen #3002, the departure endpoint and its adjacent control move. Fixed carbon #3001, the arrival endpoint/control, chemistry, all other atoms, C60, ethyl acetate, scopes and the PNG/EMF picture remain exact at binary32. The measured resulting length is about `18.00000647pt`, reflecting binary32 storage.
- The fresh-process save must equal restored byte for byte. Hashes also cover every listed raw JPEG, AX, console/JSON record and compact metadata file. Paths cannot escape this directory. JPEG magic is checked; pixels and AX behavior are not replayed.

The native files are named explicitly in `manifest.json`: restored, intentionally stretched and fresh reopened. An edited result cannot be substituted for restored, and the authored reference cannot be relabelled as an actual native save. Historical source translations/remaps are documented in pinned `reference.json`.

## Records and qualification

`metadata/` contains **authored compact summaries of frozen actual records**, explicitly marked as such. These are not raw signatures or original build/GUI logs. Their original record hashes bind the retained private chronology; their packet-relative basis paths bind the published summaries. The artifact basis records the fresh default build's 16 owned non-test artifacts, 17 depfiles and ROOT's 587 checked included inputs. A source hash dictionary is distinct from the full-tree manifest. The unique review package changes only its `Info.plist` and executable signature; the other seven package payloads remain exact. Recorded ad-hoc signature verification is not notarization.

The final native basis records floating/docked NMR, atom-number/ppm/Both/off labels, descending-ppm sticks, marker selection, palette operations without history changes, combined opacity Undo/Redo, standalone exposed-rim visibility, fixed-carbon Stretch, tab reopen and normal quit/fresh-process reopen. It preserves the shared recovery banner and unsuccessful horizontal-wheel pan limitation. These observations belong to ROOT's actual review; the Python checker does not establish them independently.

The source-head CI snapshot is copied byte-exact to `records/ci-source-terminal-20261011-v1.json`: **21 success / 4 skipped, all 25 terminal**, at exact source `29157b3…`. A later evidence-only commit requires its own CI. The compact checks basis separately records 196 unique focused tests, one combined MOL/EMF append test and one normally ignored/manual debug query measurement, plus eight normal hooks. Those scoped results are not a full-workspace test count or a performance gate.

All raw copies stay under the repository's existing `tests/fixtures/**` formatter exclusion. Copied originals must not be reformatted, recompressed or normalized. Authored metadata is supplied in normal formatter-compatible form. No new broad formatter exclusion is required.

## Limits and corruption controls

This is a finite byte/JSON/reference audit. It does not execute Rust loaders or the app, replay Undo/Redo, verify OS signatures, authenticate source/build attestations, inspect pixels, predict NMR, establish general chemical equivalence or measure performance. Static padded component boxes are not renderer ink bounds. Session-state absence in saved JSON does not by itself prove a GUI action. Separate source, compiled-test, artifact and native receipts qualify those results.

Meaningful temporary controls can alter a raw JPEG byte; a native target, gap, cubic control, cage depth or NMR field; a supposedly fixed Stretch component; reopened bytes; or the declared executable identity. Structural controls update their temporary file hash and correlation fields first, so they must fail on semantics rather than a stale hash. Originals are never modified. The private validation receipt reports actual executions; no mutated file belongs in the frozen packet.
