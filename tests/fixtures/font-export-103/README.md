# Variable-font export regression (#103)

These development fixtures reproduce #103 and verify the compatible weight
patch in `vendor/usvg`, `vendor/svg2pdf`, and `src/style/font.rs`. They are a
**controlled backend test**, not a native Fedora editor reproduction.

The pinned Noto Sans JP font has a `wght` axis with range 100–900 and default
100. With only this face in the font database, the unpatched
resvg/usvg 0.45.1 renders SVG normal/400 and bold/700 text as Thin (100). The
resvg/usvg 0.48.1 comparison backend, for the original TrueType cases, matches
independently generated static 400 and 700 controls, respectively. The local
0.45.1 patch applies the same
weights while retaining the existing selectable PDF stack. See the probe in
[`tools/font-export-probe`](../../../tools/font-export-probe/README.md).

## Contents and provenance

- `variable-default-100.subset.ttf`: the original variable font subset to H/N/O,
  retaining its non-400 default. Family renamed to `ReShiki Font Export Fixture`.
- `static-100.subset.ttf`, `static-400.subset.ttf`, `static-700.subset.ttf`:
  independent FontTools instances of that subset, with the same renamed family.
  They are rendered in separate font databases to avoid selection ambiguity.
- `source.json`: immutable font source revision, URLs, SHA-256 values, axes,
  generation settings, copyright and fixture checksums.
- `OFL.txt`: complete, unmodified upstream SIL Open Font License 1.1.
- `issue.svg`: unmodified SVG attached to
  [issue #103](https://github.com/Ameyanagi/ReShiki/issues/103), from
  [this attachment](https://github.com/user-attachments/assets/31478ca4-b6d3-4179-aca2-ca0b89ccd360).
  SHA-256: `9c7193a0c4fd78e75d57fa3ea80004a7123715ea1d20ad33629f8e09a393c18c`.
  It requests `Noto Sans CJK JP`, normal, and contains O/N/H/O text. The probe
  changes only the family and requested weight in memory. It preserves the
  physical size, geometry, and baselines, and renders at 1200 dpi (737 × 889).
- `evidence-2026-10-02.json`: observed results and the limits of the conclusion.
- `cff2-variable-default-100.subset.otf` and `cff-static-*.subset.otf`: a
  separately pinned CFF2 source and independent CFF1 static instances. Vertical
  tables are omitted because this fixture covers horizontal chemical labels.
- `source-cff2.json`: CFF2 source, generation settings and checksums, under the
  same OFL. `cff2-outline-controls.tsv` records independent unrounded FontTools
  outlines; static CFF instancing rounds relative deltas cumulatively.
- `evidence-fixed-2026-10-02.json`: patch results on macOS ARM64 and Arch Linux
  x86_64, PDF verification, and remaining native validation.

Font copyright: © 2014–2021 Adobe (http://www.adobe.com/), with Reserved Font
Name 'Source'. The modified fixtures use a different family name, retain the
font's copyright/license metadata, and are distributed under the OFL, rather
than the ReShiki source-code license. These small fonts are development-only
fixtures and are not loaded into the user's system or application font database.

The upstream sources are Noto Sans **JP**, used as controlled substitutes for
the reporter's Noto Sans **CJK JP**. The reporter's font bytes, package version,
face index, and original native drawing were not supplied. The fixture supports
the variable-default cause; it does not establish the reporter's exact installed
font or reproduce the native editor, text cache, or fallback resolver. The PDF
probe separately verifies actual embedded instances and selectable text.

## Regeneration

Use a separate environment with `fonttools==4.61.1`. Download the immutable
source-font and license URLs recorded in `source.json` and `source-cff2.json`
to local files; then run:

```sh
python tools/font-export-probe/prepare_fixture.py \
  --source /path/to/NotoSansJP-VF.ttf --license /path/to/LICENSE \
  --cff2-source /path/to/NotoSansJP-VF.otf
```

The script verifies the source-font and license checksums, preserves the
original timestamps, subsets H/N/O, creates static controls with FontTools, renames the family, and
records output hashes. A second generation was byte-identical for every font,
the OFL file, and `source.json`. No FontTools or Python dependency is added to
ReShiki's runtime.

## Remaining acceptance

The `current fixed` probe passes against the locally patched production stack.
Native Fedora validation still needs the actual font identity, current ReShiki
build, saved drawing, UI/PNG comparison at 100% and the reported 422% zoom, and
font/fallback controls. The probe verifies application face metrics, SVG backend
outlines, static controls, selectable PDF text and mixed Regular/Bold PDF
resources. Native UI, clipboard and printing acceptance follows the combined
application build; no platform-native result is inferred from the probe.
