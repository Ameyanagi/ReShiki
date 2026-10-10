# Drawing-tool defaults visual fixtures

These original ReShiki methanol fixtures accompany
[the visual review](../../../docs/drawing-tool-defaults-review.md) for
#250/#90/#249. Base: `51fa0991`; implementation: `81227315`.

- `methanol-click-input.rsk` is the unchanged native input used in both actual
  desktop lone-pair clicks. It contains two atoms, one bond and no marks.
- `methanol-click-after-desktop.rsk` is the actual candidate native Save As
  after the click, Undo and Redo review. It adds one O lone-pair annotation;
  the skeletal C's computed `label_h` cache refreshes from 0 to 3.
- `properties-comparison.json` records the actual native Properties panels,
  graph invariants, exact saved mark and disclosed cache exception.
- `orbital-methanol-*.rsk` are frozen inputs used without modification in
  baseline and candidate PNG exports. Their node, axis, phases, layers, atom
  and bond data are identical before and after; only the renderer changes.
- `orbital-snap-{on,off}-desktop.rsk` are both actual candidate native Save As
  results from the same pointer drag with the control on/off. Their scratch
  source directory was named `before`, but neither is a baseline result.
  Original orbital 3 is preserved; only new orbital 4's node and relative
  axis differ. The release endpoints match within 1e-5 world units.
- `evidence-provenance.json` records original capture/export/input hashes,
  sizes, encodings and exact-source candidate provenance.

Reopen the clean click input and replay the action for the new default.
Reopening an already saved old mark intentionally preserves its old offset.
Use JACS/ACS Arial 10pt, light canvas, F8 off, Properties visible, 250% zoom and
the fixed camera documented in the review. Choose Lone pair, click O once,
then deselect on blank canvas. Native file comparison excludes only atom
marks and computed `label_h`; all other fields must match. The new mark is
checked independently against the exact-head model replay.

For exports, run the preserved baseline or candidate application with
`--cli convert INPUT.rsk -o OUTPUT.png --receipt`. The default 1200 dpi
preserves the configured canvas background. The experimental `--cli render`
preview uses white paper and is not the dark-background publication path.

All screenshots are untouched original captures. The provider saved JPEG
bytes under `.png` names for desktop captures; publication copies use `.jpg`
without recompression. Renderer publication images are actual PNG.
The smaller export gallery demonstrates rendering; native desktop review
separately verifies menus, clicking, modifiers and Undo/Redo.

Fixture molecules, orbital geometry and ReShiki evidence are original work
contributed under MIT OR Apache-2.0. No third-party chemical dataset or artwork
is bundled. ChemDraw primary help informed relative-offset behavior only;
there is no measured numerical ChemDraw lone-pair default comparison.
