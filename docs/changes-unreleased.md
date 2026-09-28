# Unreleased changes

The Windows dropdown fix is recorded in [ReShiki 0.9.1](changes-0.9.1.md). Publisher-style, theme, clipboard and aromatic-fusion updates are recorded in [ReShiki 0.9.0](changes-0.9.md).

## Safe regular-ring placement (under review)

By @Ameyanagi in [PR #75](https://github.com/Ameyanagi/ReShiki/pull/75).

Regular rings reject saturated/protected attachment sites and coincident duplicate
vertices without changing your drawing, selection, or Undo/Redo history. Extra
overlapping vertices are rejected rather than merged; valid atom sharing and
outward bond fusion remain available. See the [graph counts, saved fixtures, and
capture details](changes/regular-ring-safety.md). The three rejection cases,
valid outward fusion, and Undo/Redo behavior were verified in the combined macOS
integration application; capture provenance is recorded with the evidence.

| Before                                                                                                                              | After                                                                                                                                           |
| ----------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| ![Before: regular-ring placement creates carbon valence six and four coincident atom pairs.](images/regular-ring-safety/before.png) | ![After: invalid regular-ring placements are rejected while the original molecular graphs remain intact.](images/regular-ring-safety/after.png) |

## Website videos and reusable color palettes

By @Ameyanagi in [PR #59](https://github.com/Ameyanagi/ReShiki/pull/59).

The home page now includes the full ReShiki tour, with a separate release-highlights video in the 0.9.1 notes. The README links to both videos.

The new [color palette reference](https://reshiki.com/guide/color-palettes/) compares all 118 elements in Presentation, Pastel and Jmol, in both light and dark mode. Copy exact RGB or derived OKLCH values, or download JSON, CSV, CSS variables and native theme files. A regression test checks the published data against ReShiki’s color code.

The palette panel follows its Light / Dark toggle independently of the website interface. Select an element to inspect its color, or choose paper, ink and ring-fill swatches.

| Light canvas                                                                                                                        | Dark canvas                                                                                                                                   |
| ----------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| ![Presentation palette on white paper, with selenium selected and its RGB and OKLCH values shown.](images/color-palettes/light.png) | ![Presentation palette on black paper, with selenium selected and the surrounding panel matching the canvas.](images/color-palettes/dark.png) |
