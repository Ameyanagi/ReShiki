# Unreleased changes

The Windows dropdown fix is recorded in [ReShiki 0.9.1](changes-0.9.1.md). Publisher-style, theme, clipboard and aromatic-fusion updates are recorded in [ReShiki 0.9.0](changes-0.9.md).

## TBDPS and OTBDPS protecting groups

Under review — by @Ameyanagi in [PR #69](https://github.com/Ameyanagi/ReShiki/pull/69).

Enter TBDPS or OTBDPS as real protecting groups, preserving chemistry when expanding, saving, and exchanging drawings. TBDPS attaches through silicon; OTBDPS attaches through oxygen and reverses its label to TBDPSO when the bond is on the right. Both are available from atom-label entry and the chemical-abbreviation presets.

![TBDPS and OTBDPS with left and right attachments, expanded structures, and matching molecular formulas.](images/tbdps-abbreviations.png)

The image is application renderer output; [reproduction and molecular checks](changes/tbdps-abbreviations.md) document the full graph and editable exchange.

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
