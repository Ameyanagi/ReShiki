# Retained (+)-lactic acid and the local naming dock

The compact **Import → Name** dock previously rejected `(+)-lactic acid` because
the strict OPSIN Rust adapter refuses ignored stereochemistry. Optical rotation
alone cannot assign an absolute configuration. This particular complete
retained name is nevertheless unambiguous: [ChEBI:422](https://www.ebi.ac.uk/chebi/CHEBI:422)
identifies it as `(2S)-2-hydroxypropanoic acid`, graph `C[C@H](O)C(=O)O`.

ReShiki now normalizes only that complete retained name to the explicit S name,
parses it with the same bounded local Rust worker, and requires exact graph and
absolute-stereo identity before insertion. The result identifies the retained
name's source. **Add name below** preserves the entered name; **Show chemical
name** generates `(2S)-2-hydroxypropanoic acid` with the existing local rules.
There is no general `+`/`−` to R/S rule, network request, API, Java or foreign
runtime. Other optical names, salts, derivatives and conflicting descriptors
continue through strict rejection. Existing process, input/output and graph
limits are unchanged; all 191 OPSIN crate files are unchanged.

## Source-qualified native evidence

Before captures use source tree `11b50562b9f625665a244679fc1b42cb83c3d25b`;
the fix and its native captures use `22a60b33376bfbcb9e2af32880b128f27877ec65`.
These are staged source-tree identifiers. The after app's ad-hoc-signed
executable is `e7c850e53616c0d57058790d9e26392992b5ddac6b82da1f13f95cb91c186ecb`.
The [packet](../../tests/fixtures/retained-plus-lactic-acid/README.md) contains
source/artifact summaries, unchanged Root receipts, original captures and saves.
Raw JPEGs and AX captures were copied without recompression or reconstruction.

| Input/action                                    | Before                                                                                                         | After                                                                                                                                                                                                                                                                           |
| ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `(+)-lactic acid`, Import → Name → Insert, 250% | [Matched empty-canvas rejection](../images/retained-plus-lactic-acid/12-before-fix-empty-canvas-rejection.jpg) | [S structure and entered-name caption](../images/retained-plus-lactic-acid/02-plus-lactic-inserted.jpg), starting from an empty tab                                                                                                                                             |
| One Undo / Redo of that import                  | —                                                                                                              | [Undo clears graph and caption](../images/retained-plus-lactic-acid/03-plus-lactic-one-undo.jpg); [Redo restores both](../images/retained-plus-lactic-acid/04-plus-lactic-one-redo.jpg)                                                                                         |
| `(+)-butan-2-ol`, Insert                        | Strict optical qualifier remains unsupported                                                                   | [Rejected with existing S-lactic drawing intact](../images/retained-plus-lactic-acid/05-unrelated-optical-name-rejected.jpg)                                                                                                                                                    |
| Show chemical name / fresh native reopen        | —                                                                                                              | [Generated name](../images/retained-plus-lactic-acid/06-generated-s-name.jpg); [retained-name reopen](../images/retained-plus-lactic-acid/07-fresh-process-retained-name.jpg); [generated-name reopen](../images/retained-plus-lactic-acid/09-fresh-process-generated-name.jpg) |

The primary comparison uses the same complete name, Insert action, Add name
below on, collapsed Preview/Details, initially empty graph, captured 250% scale,
light theme, JACS / ACS style, Arial10 and original 2560×1704 pixels. To obtain the
old empty view at 250%, Root inserted ethanol and completely undid it before
the rejected request; its redo history remains. The new empty tab started at
100% and successful insertion automatically fitted to 250%. This setup
difference is recorded, not hidden. The immutable original receipt abbreviates the style as ACS/ACS; both untouched screenshots show JACS / ACS. The [earlier ethanol-canvas rejection](../images/retained-plus-lactic-acid/before-unsupported-lactic-name.jpg)
remains supplemental. Images 02–05, 07 and [clean completed state 11](../images/retained-plus-lactic-acid/11-completed-retained-name-clean.jpg)
are at 250%; 06/09 are at 220%. The long generated caption wraps midword into two fully visible lines at that zoom. Its complete text is proved by saved JSON and tests.
A finite data checker does not perform a pixel/text-layout oracle.

Root's actual native trial inserted six atoms, five bonds and one S center,
with a linked caption in one Undo/Redo operation. The unrelated optical request
left the saved retained-name document byte-identical. Normal quit and fresh
process reopen preserved both original and generated captions; the two second
saves are byte-identical to their first saves. The raw Root receipt distinguishes
unqualified capture 08, a failed dynamic file-list selection that opened a log
as invalid SMILES; Save was cancelled. It is not used as reopen evidence.

## Inherited names and reaction maps

The same local dock and molecule context action also retain the broader
source11b trial: [ethanol caption reopened](../images/retained-plus-lactic-acid/inherited-23-ethanol-caption-fresh-reopen.jpg),
three paired maps applied to `CCO>>CC=O`
([16](../images/retained-plus-lactic-acid/inherited-16-reaction-map-applied.jpg)),
manual map 2 → 27 ([18](../images/retained-plus-lactic-acid/inherited-18-reaction-manual-map-27.jpg)),
one Undo back to 2 ([19](../images/retained-plus-lactic-acid/inherited-19-reaction-manual-map-undo.jpg)),
map-label dragging without atom movement
([20](../images/retained-plus-lactic-acid/inherited-20-reaction-map-label-drag.jpg)),
and [fresh reaction native reopen](../images/retained-plus-lactic-acid/inherited-22-reaction-map-fresh-reopen.jpg).
Those captures/fixtures remain qualified to the earlier tree; 3,560 of its
3,565 paths are byte-identically inherited by the five-path retained-name fix.
Mapping 16/18/19/20 use 138%, reaction reopen uses 131%, ethanol reopen 250%.
The already aligned reaction's Align action was a no-op, not evidence of a
changed layout. Root's raw record also retains the interrupted earlier ethanol
Redo attempt; its subsequent independent clean trial is the qualified one.
Native format is 25; the existing default document version remains 15.

## Checks and limits

The previous 559 checks remain qualified to source11b. At source22a, 13 local
and 10 import-dock tests passed, including three new retained-name regressions:
precise S versus R/unspecified/charge/isotope/connectivity identity, actual local
forward/reverse generation, and linked-caption atomic history/error behavior.
This is 559 prior checks plus 23 rerun passes with three new cases, **not 582
distinct tests**. The ordinary default build's 19 owned artifacts and their
source/resource inputs were checked; cached artifacts were qualified separately
from the three newly compiled artifacts. No unchanged OPSIN corpus rerun is
claimed for this five-path fix. Current publication hooks and exact-head CI
remain separate pending gates until Root records their actual outcomes.

The stdlib-only packet checker verifies original byte hashes, the finite native
S-lactic graph and stereo-neighbor parity without trusting CIP/H display caches,
linked caption IDs/text, and unchanged drawing data after name regeneration.
It reconciles the fresh-save record and original 23-test logs. Corruption controls
exercise those data checks. It does not reproduce GUI actions, pixels, optical
measurements, signed-build attribution, general CIP or comprehensive IUPAC/PIN
coverage. Broader reaction-map/ethanol files are byte-preserved historical
fixtures rather than a new general mapping oracle.

## Fresh cleanup package acceptance

A separate [fresh native supplement](fixtures/names-maps-final-native-v1/README.md)
qualifies source tree `bfbce003804680bd18aa39048490b035a09975dc` and the ordinary
default review app signed as `8d96eff1…`. It preserves the original source22a
packet, its matched comparison and test-count qualification. Root repeated local
S-lactic insertion with linked caption and one Undo/Redo, context Hide/Show,
Save and fresh-process reopen without helper overrides. Both caption saves are
byte-identical to source22a. Fresh process 42659 followed verified exit of 41162.

The reopened reaction's manual map 2 → 27 changes only reactant atom ID 2's
`map_num`; one Undo restores the complete baseline bytes. The supplement keeps
six original JPEG/AX pairs, five native saves and compact unchanged receipts.
[Caption hidden](fixtures/names-maps-final-native-v1/captures/06-chemical-name-hidden.jpg)
and [local name shown](fixtures/names-maps-final-native-v1/captures/07-local-generated-name.jpg)
share 250% framing; fresh name reopen uses 220%, maps use 128%. Complete captions
are checked in native data; visible midword wrapping is not clipping. This is
fresh packaging/native acceptance, not a rerun of all 23 GUI cases or a new
unique chemical-test count. Normal source commit `699fa744` passed all eight hooks after a feature-status
table-formatting correction; cell content/alignment, all 690 compiled inputs and
78 resources remain exact. The old raw receipt retains its historical pending
remark and first formatter failure. Exact publication-head CI remains pending.
