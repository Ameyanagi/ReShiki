# Unreleased changes

The Windows dropdown fix is recorded in [ReShiki 0.9.1](changes-0.9.1.md). Publisher-style, theme, clipboard and aromatic-fusion updates are recorded in [ReShiki 0.9.0](changes-0.9.md).

## Stable canvas during selection — under review

By @Ameyanagi in [PR #74](https://github.com/Ameyanagi/ReShiki/pull/74), following @HiroYokoyama's proposal in [issue #61](https://github.com/Ameyanagi/ReShiki/issues/61).

Selecting atoms and opening object properties keeps the drawing steady, making double-click molecule selection reliable. Bonded-movement controls stay in the existing scrolling context row. Explicit Fit, inspector toggles, window resizing, pan, and zoom keep their behavior.

This change is under review and unreleased. Matched examples come from application-renderer and pointer-event checks. Real macOS desktop checks also passed on combined source `446331e`, including the minimum window size, inspector visibility changes, and the optional object toolbar. [Reproduction steps, fixtures, measurements, and exact capture sources](changes/selection-canvas-stability.md).

| Before                                                                                                              | After                                                                                                                         |
| ------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| ![A second click misses the nitrogen after the canvas shifts](images/selection-canvas/before-1280-double-click.png) | ![Double-click selects the whole molecule while the drawing stays fixed](images/selection-canvas/after-1280-double-click.png) |

## Windows GPU rendering and nightly builds

By @Ameyanagi in [PR #68](https://github.com/Ameyanagi/ReShiki/pull/68).

Windows builds include WGPU with a Tiny Skia fallback. CPU-only environments keep
the direct software renderer, and explicit renderer overrides remain available
for testing. The [validation record](windows-gpu-validation.md) includes Windows
VM measurements and real Radeon GPU rendering on Linux; physical Windows GPU
performance remains unmeasured.

Development PRs target `main`. Successful nightly builds publish unsigned portable
prereleases for all six platforms, with unique versions and checksums. Stable
releases use version tags on tested commits in `main`.

## Website videos and reusable color palettes

By @Ameyanagi in [PR #59](https://github.com/Ameyanagi/ReShiki/pull/59).

The home page now includes the full ReShiki tour, with a separate release-highlights video in the 0.9.1 notes. The README links to both videos.

The new [color palette reference](https://reshiki.com/guide/color-palettes/) compares all 118 elements in Presentation, Pastel and Jmol, in both light and dark mode. Copy exact RGB or derived OKLCH values, or download JSON, CSV, CSS variables and native theme files. A regression test checks the published data against ReShiki’s color code.

The palette panel follows its Light / Dark toggle independently of the website interface. Select an element to inspect its color, or choose paper, ink and ring-fill swatches.

| Light canvas                                                                                                                        | Dark canvas                                                                                                                                   |
| ----------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| ![Presentation palette on white paper, with selenium selected and its RGB and OKLCH values shown.](images/color-palettes/light.png) | ![Presentation palette on black paper, with selenium selected and the surrounding panel matching the canvas.](images/color-palettes/dark.png) |
