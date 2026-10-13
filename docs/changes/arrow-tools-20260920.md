# Arrow tools: original checks (2026-09-20)

These checks describe the first arrow build, not the current toolbar. The
[0.6 release walkthrough](../release-0.6-validation.md) covers the current
toolbar and rendering, and the [arrow page](../arrows.md) describes how arrows
work today.

## Fixtures

Saved interchange fixtures cover arrow handles, head shapes and supported arrow
variants. The [fixture directory](../../tests/fixtures/) contains the editable
CDXML records used by regression tests.

## Desktop checks

Desktop checks in an isolated ReShiki QA app:

- Drew a forward arrow and bent it with the middle handle; Undo/Redo restored
  each complete edit.
- Changed it to a blue half-head, reversed it, flipped the bend and dragged an
  endpoint. The inspector preview tracked the actual curve.
- Drew an equilibrium arrow, set reverse length to 0.55 and separation to 3 pt,
  and saved through the save dialog. The `.rsk` file kept both controls and all
  styling.
- Exported SVG/PDF through the GUI and visually checked the rendered PDF.
  Attempting CDXML with the unequal equilibrium produced an explicit
  unsupported-combination message.
- Restarted the standalone QA bundle, reopened the saved drawing, and checked
  that the controls and colors survived. Imported the externally saved
  seven-arrow gallery and saved it as a version 7 `.rsk` file; the bundled
  chemistry worker also preserved both custom arrows alongside ethanol.
- Changed the text style to 20 pt, bold and red, then used New. The toolbar
  returned to Arial 10 pt and black; Arrow returned to Forward with a 0.6 pt
  stroke.

## Tests

Validation: **96 Rust tests and 26 Python tests**, plus formatting and Clippy.
Regression coverage includes bend hit testing, pointer editing,
bounds/lasso/alignment, transformations and duplication, hollow heads, unequal
equilibrium, `.rsk` persistence, Undo/Redo, new-document defaults, chemistry
preservation, supported CDXML exchange, and rejected lossy combinations. Local
screenshots, exported figures, saved drawings and packaging logs are under
ignored `artifacts/arrow-qa-20260920/`.
