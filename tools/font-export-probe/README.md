# Font export probe

This small, separate Cargo workspace reproduces
[ReShiki #103](https://github.com/Ameyanagi/ReShiki/issues/103) without compiling
the application or loading host fonts. It pins the application's resvg/usvg
0.45.1 backend and, behind the `oracle` feature, resvg/usvg 0.48.1 for comparison.
The root application's manifest and lockfile are unchanged.

Each case renders a variable font and three independent static FontTools
controls in separate one-face font databases. It compares **every RGBA pixel**,
checks that all three controls differ, and saves PNGs plus a TSV report. The two
cases are large H/N/O labels and the supplied issue SVG at 1200 dpi. Both use
unset, normal, 400, bold, and 700 weights. No visual snapshot is regenerated as
an expected result.

From the repository root:

```sh
# Verify fixture structure and static control weights.
cargo test --locked --manifest-path tools/font-export-probe/Cargo.toml -j 2

# Confirm the current bug: all 10 variable-font cases match Thin (100).
cargo run --locked --manifest-path tools/font-export-probe/Cargo.toml -j 2 -- \
  current bug /tmp/font-export-103-current

# Acceptance command. This must FAIL until the production backend is fixed.
cargo run --locked --manifest-path tools/font-export-probe/Cargo.toml -j 2 -- \
  current fixed /tmp/font-export-103-acceptance

# Independent renderer comparison; this does not upgrade the application.
cargo run --locked --manifest-path tools/font-export-probe/Cargo.toml \
  --features oracle -j 2 -- oracle fixed /tmp/font-export-103-oracle
```

`bug` and `fixed` are distinct expectations. A successful `bug` run confirms the
known failure; it must never be reported as a passed fix. `fixed` requires normal
and 400 to equal static 400, and bold and 700 to equal static 700. An unmatched
control produces a nonzero exit status. The TSV's `dark_coverage_sum` excludes
white background pixels; exact pixel comparison is the acceptance criterion.

Source, license, generation and observed evidence are in
[`tests/fixtures/font-export-103`](../../tests/fixtures/font-export-103/README.md).
The font is a pinned Noto Sans JP substitute, not the reporter's actual Fedora
font file. The oracle comparison is limited to this backend fixture; it does
not validate ReShiki's editor, application metrics, PDF, or native fallback.

## Why this commit does not bump resvg

ReShiki passes a single `usvg::Tree` into svg2pdf 0.13.0, which requires the
resvg/usvg 0.45 line. Bumping only the PNG dependency would introduce incompatible
tree types and leave the PDF path and application face metrics unresolved.
Variable weight must reach shaping, metrics, outlines, and the PDF font-resource
identity and subsetting. The PDF path currently keys resources by face ID and
embeds the default instance, so mixed weights from one variable face need
separate resource identities. No speculative production migration is included.

This is the evidence and regression stage of the issue plan. The remaining
production route should be tested with these controls and then verified on
native Fedora before #103 can be considered fixed.
