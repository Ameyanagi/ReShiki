# Editable graphics: original checks (2026-09-20)

These checks describe the first graphics build. The
[graphics page](../graphics.md) describes how shapes and pen paths work today.

## Desktop checks

Implemented and desktop-checked on 2026-09-20. The desktop check drew a filled
rectangle behind aspirin, selected its bond through the background, added a
separately colored ellipse, dragged a Bézier control point, and bracketed the
aromatic ring. The drawing was saved and reopened as `.reshiki`, exported
through the CDXML save dialog, opened and saved by an external drawing
application, and reopened in the standalone ReShiki app. The externally saved
fixture is under `tests/fixtures/`; local screenshots and saved files are under
ignored `artifacts/graphics-qa-20260920/`.

## Tests

This graphics update passed 59 Rust and 18 Python tests. New coverage exercises
every shape's transforms, hit testing, point edits, copy/remapping, history,
color pixels in raster export, vector export, cleanup preservation and CDXML
geometry/layers. It also checks graphics-only files, supported legacy objects,
an externally saved interchange file, and explicit errors for unsupported
styles and object types. Formatting and Clippy pass. At that milestone, the
standalone bundle passed signature verification and imported the externally
saved fixture from `/tmp` with `RESHIKI_ROOT=/nonexistent`, using its bundled
worker.
