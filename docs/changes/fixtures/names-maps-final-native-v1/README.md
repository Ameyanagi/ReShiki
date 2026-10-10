# Fresh names/maps native acceptance supplement

This is a fresh-package Mac GUI acceptance supplement for source/index tree
`bfbce003804680bd18aa39048490b035a09975dc`, native format 25. It leaves the
[source22a retained-name packet](../../retained-plus-lactic-acid.md) and its
matched pre-fix comparison unchanged. It adds no unique chemical test count and
does not claim that all 23 earlier scoped cases were rerun through the GUI.

The ordinary default GUI build was packaged as the uniquely identified
`dev.reshiki.names-maps-cleanup-review-20261011-v1` app. The signed executable is
`8d96eff1c44f124b2364da68a39e9808c5dc1889066dc980249fb88eba79477d`;
its retained unsigned default binary is
`410bbb3dce4e8f2ada613d2d157389e2f78e1f7b599c4e249fe6fe2b0d7e4d3e`.
The development helper (`9e770329…`) was retained separately, not shipped.
One native executable also hosts the local worker. Root's recorded preflight
verified the signature, unique bundle ID and absence of helper/Python overrides
in LaunchServices. This debug review package is ad-hoc signed, not notarized.

## Recorded native checks

Root exercised `(+)-lactic acid` insertion with six atoms/five bonds and a linked
caption, one Undo/Redo, right-click Hide/Show, native Save, normal quit and
fresh-process reopen. Old process 41162 was absent; the new process was 42659.
The generated caption is `(2S)-2-hydroxypropanoic acid`. Both retained-name and
generated-name saved files are byte-identical to the qualified source22a files.
The fresh-process second save preserves the generated file exactly.

Root reopened the mapped `CCO>>CC=O` reaction, changed reactant atom ID 2's map
from 2 to 27 and used one Undo. The five native fixtures preserve all IDs,
connectivity, stereo, coordinates, caption links, label styling/offsets and
reaction roles except that one deliberate `map_num` edit. Undo restores the
complete baseline native bytes. This supplement does not repeat automatic
mapping, alignment, all optical-name rejection cases or cross-platform GUI trials.

| Original capture                                                                        | Recorded state                                                         | Scale |
| --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- | ----- |
| [02](captures/02-plus-lactic-inserted.jpg)                                              | Inserted S graph and entered-name caption; selected after insertion    | 250%  |
| [06](captures/06-chemical-name-hidden.jpg) / [07](captures/07-local-generated-name.jpg) | Caption hidden / generated name shown, same viewport                   | 250%  |
| [08](captures/08-fresh-process-generated-name.jpg)                                      | Fresh-process reopen, six atoms/five bonds, S SMILES and clean history | 220%  |
| [10](captures/10-reactant-map-27.jpg) / [11](captures/11-reactant-map-undo.jpg)         | Manual reactant map 27 / one Undo restores map 2                       | 128%  |

These six JPEGs are untouched 2560×1704 captures. This is a comparison of states
in the current app, not a new preimplementation comparison. Matching original
AX captures are preserved as captured; 06/07 have identical AX bytes. They are
not reconstructed canvas trees. The long generated name wraps midword into two
fully visible lines in 07/08. Complete caption text is checked in the native
files. No pixel/text-layout oracle or glyph-clipping claim is made. The visible
recovery banner was left untouched.

## Portable verification

With Python 3.9 or newer and no third-party modules, from the repository root:

```sh
python3 docs/changes/fixtures/names-maps-final-native-v1/verify_native.py
python3 docs/changes/fixtures/names-maps-final-native-v1/verify_native.py --self-test
```

The checker uses only paths inside this packet. It hashes all raw copies,
recognizes the finite S-lactic reference `C[C@H](O)C(=O)O` independently specified
by [ChEBI:422](https://www.ebi.ac.uk/chebi/CHEBI:422), and ignores cached CIP/H
display values. It verifies linked caption targets/text, unchanged graph and
positions between both captions, the exact whole-document map edit and
byte-identical Undo. Ten corruption controls exercise meaningful data/byte
failures. This is not a general CIP/naming engine, public native loader, GUI
Undo reproduction, renderer test or independent build/signature attestation.
The unchanged Root receipts provide the separate recorded GUI/package basis.
The final receipt inventories all 11 original screenshot/AX pairs; this compact
packet copies the six listed above. It records focused acceptance, not all
earlier cases. Ephemeral worker child PIDs were not observed; routing is
qualified by package/source/preflight rather than a claimed child-PID capture.

`status.json` is an authored compact correlation summary; `sha256.json` lists
the raw copy hashes. Files under `raw/` retain their original JSON bytes with a
`.json.raw` suffix. The package/prelaunch/initial graph records predate GUI
completion and retain their original pending fields. The final raw
`root-native-acceptance-final-v1.json.raw` and authored status close those native
gates separately. No raw receipt is relabeled or
reformatted. `.raw`, `.rsk`, AX text and binary JPEGs are outside the formatter's
eligible source extensions; every copied text file uses LF. No new broad
formatter ignore or Git line-ending rule is proposed.

The existing test counts remain 559 prior checks plus 23 scoped passes
(three new cases and 20 reruns), not 582 distinct tests. This fresh native
supplement adds no Rust test executions. Source-commit hooks and exact-head CI
are separate from native acceptance. The unchanged final native receipt records
its historical first attempt: seven passing gates and the feature-status table
formatter failure. The newer raw `root-normal-hooks-success-v1.json.raw` records
actual commit `699fa7449b0c65e16b2f1a8435dd91da2c3ba423`, tree
`6262c40dbcdc71980f71d7f354fea64a0b97a9d1`, with all eight normal hooks passed.
`status.json` contains the authored source bridge: only column padding and
separator dash widths in `docs/feature-status.md` changed after native-tested
bfb. Table cell content/alignment and every other line are unchanged. All 690
compiled source inputs and 78 resources remain exact. Prior failed attempts
remain preserved. Exact publication-head CI is still pending.

Project creator and maintainer: @Ameyanagi. Upstream OPSIN 2.9.0, Daniel Lowe,
retains its MIT notices; aho-corasick 1.1.5 retains the documented MIT choice,
both offered license texts and COPYING. Existing original-code/contribution
terms remain unchanged. This evidence supplement preserves the existing local
Rust runtime and license terms.
