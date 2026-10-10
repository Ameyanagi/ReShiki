# ReShiki native Join acceptance

On 2026-10-10 the native operator opened the unchanged
[three-arm input](../../../../../tests/fixtures/join-three-atoms-before.rsk) in
the default-feature macOS ARM64 app, selected only central atoms 1/3/5 by marquee,
and pressed Cmd+J. This is one controlled fixture in the corrected app. The
before screenshot shows its state **before the action**, not an earlier binary
reproducing the original rejection.

## Actual actions and result

1. Open the controlled input with clean history, 250% view and keyboard drawing
   off (F8). The six-atom drawing has three single bonds.
2. Marquee only N1, C3 and O5; the selection UI reports three atoms.
3. Press Cmd+J. N1 survives with three carbon arms at the rendered-label bounds
   center. Terminal atom positions remain fixed.
4. One Cmd+Z restores the original graph and clean title; one Cmd+Shift+Z restores
   the merged graph.
5. Deselect to inspect the whole molecule, then Save As the separate native RSK.
   Close only that saved tab and reopen the exact saved file in the same running
   process. The reopened document has a clean title and disabled Undo/Redo.

[The exact saved result](join-three-arms-native-after-v2.rsk) is 2,530 bytes,
SHA256 `d73c9cd8408b89312522d0b6691fbffe4e17ce6a576a49187d7cdd01fa2f1e73`.
It has atom IDs `[1,2,4,6]`, three plain single bonds `1–2`, `1–4`, `1–6`, and no
extra annotations, arrows, graphics or groups. N1 is at
`(4.873413, 2.1815796)`, within 0.0001 world units of the actual helper result
`(4.8734130859375, 2.18157958984375)`. Its Arial 10 pt style and red RGB
`[189,43,57]` survive. Terminals remain at `(-48,-25)`, `(48,-25)`, `(0,50)`.
The whole/reopened drawing reports **C3H9N**, four atoms, three bonds and
`CN(C)C`. The immediate after-action screenshot's H3N readout describes the
selected nitrogen, not the whole molecule.

## Original visual evidence

All six published images are byte-exact 2560 × 1704 JPEG captures at 250% with
keyboard drawing off. They retain application chrome, cursor highlights and
Recovery draft / Update available notices. Nothing has been cropped or retouched.

| Capture                                                                                                     | What it establishes                                                   |
| ----------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| [Before the action](../../../../images/join-atom-merge/macos-join-three-arms-before-v2.jpg)                 | Six atoms and three bonds in the corrected app before Cmd+J.          |
| [Three central atoms selected](../../../../images/join-atom-merge/macos-join-three-central-selected-v2.jpg) | The actual three-atom selection, including handles.                   |
| [After Cmd+J](../../../../images/join-atom-merge/macos-join-three-arms-after-v2.jpg)                        | Red N with three arms and one selected atom.                          |
| [One Undo](../../../../images/join-atom-merge/macos-join-one-undo-v2.jpg)                                   | Original geometry restored; Redo becomes available.                   |
| [Redo](../../../../images/join-atom-merge/macos-join-redo-v2.jpg)                                           | Merged geometry restored.                                             |
| [Saved document reopened](../../../../images/join-atom-merge/macos-join-fresh-document-reopened-v2.jpg)     | Clean unselected result, whole-molecule properties and empty history. |

The closely spaced asymmetric input displays overlapping NH2/OH ink before the
merge. That input condition is retained to distinguish rendered-label centering
from arithmetic averaging; the final reopened result supplies the clean reusable
release image. Inspector selection, title and cursor state differ between action
captures. The raw before/reopened pair uses the same fixture and view scale; it
is not a comparison against a running baseline build.

## Serialization and producer scope

Native save normalizes `#bd2b39` to an RGB array, numeric/default fields and
explicit default `atom_labels`. Terminal computed `label_h` becomes 3; survivor
N has 0 after the graph change. Bond defaults are serialized explicitly. These
normalizations do not change terminal identities/coordinates or surviving
chemical/style state.

The frozen, uncommitted build was based on
`51fa0991da2507bb00b27c1b420e807468de6423`, with reviewed Join source changes.
[Producer summary](../validation/producer-summary.json) identifies the actual
raw/signed executable and fresh own Cargo artifacts. A later documentation commit
is not the binary producer commit. The separate private native protocol and
independent graph/visual audit are pinned by hash in that summary; their raw
accessibility/process records remain private.

This check establishes native Cmd+J, one Undo/Redo, Save As and same-process
fresh-document reopen for this macOS fixture. It does not establish a fresh
process restart, Windows/Linux ReShiki runtime behavior, 3D rules or numerical
identity with the separate ChemDraw fixture. The app uses its own rendered label
metrics and retains recomputed hydrogen labels. `evidence-inventory.json` pins
the seven unchanged originals and this authored summary.
