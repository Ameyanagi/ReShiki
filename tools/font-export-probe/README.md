# Font export probe

This separate Cargo workspace verifies
[ReShiki #103](https://github.com/Ameyanagi/ReShiki/issues/103) without compiling
the UI or loading host fonts. It uses ReShiki's local usvg 0.45.1 and svg2pdf
0.13.0 patches. An optional resvg/usvg 0.48.1 backend provides a comparison.

The unpatched 0.45.1 renderer selected the default Thin instance of a Noto font
for normal/400 and bold/700 requests. Historical exact-pixel evidence is retained
in `tests/fixtures/font-export-103/evidence-2026-10-02.json`. The production patch
carries the requested weight through metrics, shaping and outlines, and embeds
separate selectable PDF font instances for distinct weights.

Each render gets an isolated database containing only a pinned fixture face.
The matrix covers H/N/O labels and the issue's supplied SVG at 1200 dpi, for
unset, normal, 400, bold, and 700 weights, with both TrueType and CFF2 sources.
All 20 variable renders are compared with independent FontTools static controls.

Acceptance first checks exact advances and matching outline command topology.
For TrueType, points may differ by at most 0.5 font unit because static `glyf`
coordinates are rounded. For CFF2, unrounded FontTools outline coordinates must
agree within 0.002 unit; static CFF1 instancing rounds relative deltas, so it is
also retained as a separate raster/PDF control. Raster RGBA absolute difference
must remain below 2% of reference dark coverage. This is not exact-pixel equality:
ttf-parser keeps fractional variable points that static fonts round. All three
static weights must remain distinguishable, and the original Thin failure is
far outside the bound. No image snapshot is regenerated as its own expectation.

From the repository root:

```sh
# Includes the application's tests/font_export_103.rs without the UI build.
cargo test --locked --manifest-path tools/font-export-probe/Cargo.toml -j 2

# Production acceptance: saves PNGs, a TSV report, and 12 PDF cases.
cargo run --locked --manifest-path tools/font-export-probe/Cargo.toml -j 2 -- \
  current fixed /tmp/font-export-103-current

# Optional independent renderer comparison (does not upgrade the application).
cargo run --locked --manifest-path tools/font-export-probe/Cargo.toml \
  --features oracle -j 2 -- oracle fixed /tmp/font-export-103-oracle

# In a development environment with fonttools==4.61.1, pypdf==6.1.1,
# Pillow==12.1.1 and Poppler's pdftoppm/pdftotext on PATH:
python tools/font-export-probe/verify_pdf.py /tmp/font-export-103-current
```

The optional 0.48.1 comparison is exact for the ten original TrueType cases.
With the additional CFF2 corpus, its three regular-weight molecule cases exceed
this probe's static-control raster bound (2.44% versus 2%). That comparison
therefore exits nonzero; it is retained as a diagnostic, not the acceptance
criterion for the local patch. The production patch passes all 20 cases and
also matches the independent unrounded FontTools CFF2 coordinates.

The retained `bug` mode checks exact Thin output and is expected to fail with
the patched renderer. A successful historical `bug` run confirms the original
failure and must not be reported as a passed fix.

PDF validation reads every font program and ToUnicode mapping, checks separate
mixed-weight resources, exact `/W` versus embedded `hmtx` and independent static
advances, and descriptor bounds containing the actual embedded glyphs. CFF2
must embed as TrueType with CIDFontType2/FontFile2; static CFF1 controls retain
CIDFontType0/FontFile3. Both pypdf and Poppler extract H/N/O text. Poppler-rendered
PDFs are compared at 144 dpi with normalized RGB difference below 2% of control
ink. Additional independent review covered 96 and 300 dpi.

Source, full font license, deterministic generation and before/after evidence
are in [`tests/fixtures/font-export-103`](../../tests/fixtures/font-export-103/README.md).
Vendored changes and archive hashes are in `vendor/*/NOTICE-RESHIKI.md` and are
included by the existing binary-license collector. Root dependency versions
remain unchanged; no global font preferences, runtime Python requirement, or
text-to-outline fallback is introduced.

The font sources are pinned Noto Sans JP substitutes, not the reporter's actual
Fedora font bytes. The backend matrix passed on macOS ARM64 and Arch Linux
x86_64. Native Fedora/editor/clipboard/printing checks remain separate acceptance
work after the combined application build.
