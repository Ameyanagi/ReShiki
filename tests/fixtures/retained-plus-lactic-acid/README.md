# Retained (+)-lactic acid evidence

This packet preserves original screenshots, AX captures, two native saves and
passing test stdout. `source-correlation.json` is an authored compact summary;
`input-review.json` and `scoped-test-review.json` are unchanged Root receipts.
The earlier image is source tree `11b50562b9f625665a244679fc1b42cb83c3d25b`;
new captures and test logs are tree `22a60b33376bfbcb9e2af32880b128f27877ec65`.
These are staged source trees, not published commit identifiers.

From the repository root, with Python 3.9 or newer and no extra dependencies:

```sh
python3 tests/fixtures/retained-plus-lactic-acid/verify_native.py
python3 tests/fixtures/retained-plus-lactic-acid/verify_native.py --self-test
```

The checker hashes only paths inside this public packet/repository, recognizes
this finite C3H6O3 graph, and checks the native stereo-neighbor parity against the
independently specified S reference `C[C@H](O)C(=O)O` from
[ChEBI:422](https://www.ebi.ac.uk/chebi/CHEBI:422). It ignores `cip_label` and
`label_h` display caches. Both captions must link all six actual atom IDs to the
same graph, and caption generation must preserve every other native field.
It also reconciles the 13 local and 10 import-dock named passes with the original
review. The 23 passes include three new regressions and 20 reruns; they are not
23 additional distinct tests on top of the earlier 559-test qualification.

`--self-test` rejects inverted/absent stereo, isotope/charge/connectivity loss,
broken caption references, changed coordinates, stale generated text, escaping
paths and byte corruption. Its finite reference is not a general CIP or naming
algorithm. It does not reproduce parser execution, public native loading, GUI
Undo/Redo, signed-artifact provenance, optical measurements or pixel layout.
Those claims require the separately qualified source/build and Root native
observations. The initial source summary predates fresh-process acceptance; the subsequent
raw receipt and authored status supplement explicitly close that pending gate.

Raw AX files may be literal diffs/no-change captures; no full tree was
reconstructed. Raw JPEGs are uncropped and unrecompressed. Image 06 is at 220%
and wraps the long generated caption midword into two complete visible lines: the full text is proved by
native data and tests. The checker does not perform a pixel/text-layout oracle.

## Subsequent native acceptance

The unchanged raw `native-fresh-reopen-readback.json` closes the initial
preparation snapshot's pending reopen status: Root observed old process 1883
exit, new process 5041, and exact second-save hashes for both v25 native files.
The checker reconciles that record with the actual copied bytes. It validates
the recorded data, not whether the process/GUI observations independently
occurred. Raw 08 is an unsuccessful file-selection attempt, not acceptance;
its original remains private and the raw Root receipt records the limitation.

Six broader inherited screenshots/AX pairs and the original ethanol/reaction
native fixtures are byte-hashed. Their GUI scope and source11b qualification
are recorded in `native-observations-before-lactic-fix-v1.json`. This finite
semantic checker covers the new S-lactic files, not a general reaction-mapping
algorithm. The fresh22/23captures are at131%/250%; mapping16/18/19/20at138%.

The existing repository formatter rule `tests/fixtures/**` already excludes
these literal raw JSON/log/AX/native files, so no new formatter exclusion is
needed. The exact raw logs contain terminal blank lines; the private handoff
proposes only per-file Git whitespace attributes for those two logs. All copied
raw text uses LF line endings, compatible with the existing `eol=lf` rule.
No wildcard attribute or normalization of the original bytes is proposed.

## Matched old/new comparison

`matched-before-after-root-review.json` closes the pending comparison slot.
The exact old11b signed app rejected the name on an empty canvas, at250%;
the new22a signed app inserted the correct graph/caption from an empty canvas
and fitted to250%. Root established the old250%view using ethanol Insert and
one complete Undo first, leaving redo history; this differs from the new tab's
initial100%view. Both retained captures have matching input/action/control state,
style/theme and original2560×1704pixels. Earlier13withethanol is supplemental.
The checker hashes the matched images/AX and Root receipt, not their pixels or
actual UI history.
