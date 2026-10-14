# Chemical symbols and orbitals: original checks (2026-09-20)

These checks describe the first symbols and orbitals build. The
[symbols page](../scientific-symbols.md) describes how marks and orbitals work
today.

## Fixtures

Interchange fixtures in `tests/fixtures/` cover a p orbital, a free circled plus
and an atom-owned circled charge. The charge changes ammonia to ammonium and
stores its owner with a `represent` child.

## Tests

Tests cover all shapes/phases, `.rsk` and vector persistence, atom ownership
through transforms/copy, atomic rejection, history, radical/charge chemistry,
supported exchange and explicit exchange failures. The complete suite passes
102 Rust and 30 Python tests.

## Desktop checks

Desktop checks used a standalone app launched from `/tmp`: ammonia → circled
ammonium; mark repositioning with Undo/Redo; a rotated p orbital with reversed
gray/blue phases; save/reopen; vector/raster export; and NH₄⁺ → NH₃⁺ with one
unpaired electron. New was exercised after setting 18 pt bold red text, a 3 pt
graphic line and reversed phases; it restored JACS defaults. The final mark
position was adjusted to clear the label while meeting the interchange
proximity constraint.

A ReShiki-exported attached charge was opened and saved in an external drawing
application, then re-imported and checked as `[NH4+]`; the saved result is
kept under `tests/fixtures/`. A distant-position counterexample exposed loss of
the charge/ownership link, and now causes an explicit export error. Desktop
artifacts, drawings, screenshots and standalone checks are under
`artifacts/symbols-qa-20260920/`. The exported PNG was visually inspected; the
orbital phase and atom mark match the canvas.
