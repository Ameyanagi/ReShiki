# ReShiki changes to svg2pdf 0.13.0

Upstream source: https://crates.io/crates/svg2pdf/0.13.0

- Published archive SHA-256: `e50dc062439cc1a396181059c80932a6e6bd731b130e674c597c0c8874b6df22`
- Repository: https://github.com/typst/svg2pdf
- Source revision: `ccab6d7a3081f512d412dfef456caed2caf3a698`
- Upstream MIT/Apache-2.0 licenses, NOTICE and original `Cargo.toml.orig` are
  retained unchanged. The registry's local marker and dependency lockfile are
  omitted; ReShiki owns the locked dependency graph.

Local changes, copyright 2026 ReShiki contributors, under the same MIT OR
Apache-2.0 terms:

- Key font resources by face ID and requested variable weight. Mixed normal and
  bold text from one variable face receives separate embedded instances and
  subset names while retaining the existing CID/ToUnicode text mappings.
- Use the already locked subsetter 0.2.6 variable-font API to embed the selected
  static instance. Explicitly require its `variable-fonts` feature.
- Determine CID type and FontFile2/FontFile3 from the subset output: CFF2 input
  is converted by subsetter to TrueType, whereas CFF1 remains CFF.
- Read PDF widths by the subset's new glyph IDs, and use actual subset glyph
  bounds for variable instances. The original head bounds may cover only Thin.
- Make two existing `Name` lifetimes explicit for the current Rust warning set.

No text outlining fallback is used; selectable PDF text is retained. The
usvg 0.45 tree API and pdf-writer 0.12 remain in use. Tests cover TrueType and
CFF2 source fonts, separate normal/bold instances, CFF1/static controls, exact
embedded widths, bounding boxes, ToUnicode extraction, and Poppler rendering.

See `tests/fixtures/font-export-103` and `tools/font-export-probe` for source
font licenses, regeneration, the executable regression checks and evidence.
