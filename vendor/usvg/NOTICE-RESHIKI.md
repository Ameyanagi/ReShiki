# ReShiki changes to usvg 0.45.1

Upstream source: https://crates.io/crates/usvg/0.45.1

- Published archive SHA-256: `80be9b06fbae3b8b303400ab20778c80bbaf338f563afe567cf3c9eea17b47ef`
- Repository: https://github.com/linebender/resvg
- Source revision: `1b6c2fddbcbeffa8135df4323b02aaae84890907`, `crates/usvg`
- Upstream MIT and Apache-2.0 licenses, copyright comments, and original
  `Cargo.toml.orig` are retained unchanged. The registry's local marker and
  dependency lockfile are omitted; ReShiki owns the locked dependency graph.

Local changes, copyright 2026 ReShiki contributors, under the same MIT OR
Apache-2.0 terms:

- Carry the requested CSS weight through face metrics, shaping, fallback, and
  glyph outlines. Apply 400 even if the variable font's built-in default is 100.
- Expose `PositionedGlyph::variation_weight` only when the selected font has a
  `wght` axis, so PDF consumers can preserve distinct instances of one face.
- Use the selected weight for raster-glyph bounds and COLR outlines as well.

This is a bounded weight-support change on the existing 0.45 API, not a backport
of the complete later font stack. It does not add arbitrary CSS font-variation
settings or change static font matching. The upstream fixes in
https://github.com/linebender/resvg/pull/997 and
https://github.com/linebender/resvg/pull/1099 informed the investigation; the
implementation here retains rustybuzz/ttf-parser and the existing PDF tree API.

Regression fixtures, independent FontTools controls, and before/after evidence
are in `tests/fixtures/font-export-103` and `tools/font-export-probe`. The probe
also compiles the application's `tests/font_export_103.rs` without its UI build.
