# Unreleased changes

The Windows dropdown fix is recorded in [ReShiki 0.9.1](changes-0.9.1.md). Publisher-style, theme, clipboard and aromatic-fusion updates are recorded in [ReShiki 0.9.0](changes-0.9.md).

## Stable and Nightly update channels

By @Ameyanagi in [PR #82](https://github.com/Ameyanagi/ReShiki/pull/82).

Choose **Stable** or **Nightly** in **Check for updates**. ReShiki remembers the channel and preserves your automatic-check preference. Stable remains the default and uses verified installation; Nightly downloads the matching installer when available, with portable archives as a fallback, for manual installation. Switching back to Stable offers the latest stable release even when its version number is lower than the installed nightly.

![The update window offers Stable and Nightly, with a portable download and manual-install notice for the selected nightly build.](images/update-channel/nightly.png)

Application-renderer example with automatic checking disabled. Separate desktop checks verified channel persistence and the downloaded nightly archive; see the [validation record](changes/update-channel.md).

## Nightly installers and release downloads

By @Ameyanagi in [PR #85](https://github.com/Ameyanagi/ReShiki/pull/85).

Nightly builds include installers and portable archives for all six supported targets. macOS DMG and ZIP downloads are signed and notarized; Windows and Linux packages remain unsigned. Each release provides a table organized by operating system, architecture, installer and portable archive, with verified checksums. Stable remains the primary release.

**Download nightly ↗** prefers a matching published installer, with a portable fallback for older builds. Installation remains manual; installers replace the existing app. Use the portable option from **Release notes** to retain Stable in a separate folder.

![The Nightly update window offers Download nightly and explains that installation is manual, with portable archives linked from Release notes.](images/nightly-installers/nightly.png)

Application-renderer evidence and packaging checks are recorded in the [validation notes](changes/nightly-installers.md).

## ChemDraw arrow exchange

By @Ameyanagi in [PR #80](https://github.com/Ameyanagi/ReShiki/pull/80).
Keep arrows visible and editable when copying reaction drawings
between ReShiki and ChemDraw. The [verification record](changes/chemdraw-arrow-exchange.md)
includes actual clipboard results, matched returned-file renders and the
explicit limit on external reaction-role metadata.

![Editable arrow retained after the actual ChemDraw round trip.](images/chemdraw-arrows/after.png)

## ChemDraw caption exchange

By @Ameyanagi in [PR #79](https://github.com/Ameyanagi/ReShiki/pull/79).

Formula captions remain editable text when copied into ChemDraw, and binary
CDX preserves their line spacing. Actual macOS clipboard round trips retain
all six tested molecules and twelve captions. The
[validation record](changes/chemdraw-caption-exchange.md) documents the captured
files and the remaining caption-position and legacy-file limitations.

## Adjustable arcs

By @Ameyanagi in [PR #76](https://github.com/Ameyanagi/ReShiki/pull/76), requested by @rlavendomme in [#67](https://github.com/Ameyanagi/ReShiki/issues/67).

Draw adjustable elliptical arcs with 90°, 120°, 180° and 270° presets, precise start/sweep controls and draggable endpoints. A 360° sweep completes the ellipse; Shift drawing creates circles. Existing native arcs keep their appearance, and CDXML retains editable curve paths. The general pen tool remains separate.

![Four arc presets, a fractional sweep, a full circle, an affine transformed arc and a legacy half ellipse](images/adjustable-arcs.png)

[Example drawings, compatibility notes and completed desktop review](changes/adjustable-arcs.md).

## Safe regular-ring placement

By @Ameyanagi in [PR #75](https://github.com/Ameyanagi/ReShiki/pull/75).

Regular rings reject saturated/protected attachment sites and coincident duplicate
vertices without changing your drawing, selection, or Undo/Redo history. Extra
overlapping vertices are rejected rather than merged; valid atom sharing and
outward bond fusion remain available. See the [graph counts, saved fixtures, and
capture details](changes/regular-ring-safety.md). The three rejection cases,
valid outward fusion, and Undo/Redo behavior were verified in the combined macOS
integration application; capture provenance is recorded with the evidence.

Valid neutral-phosphorus attachment, including drawings with semantic attachment
nodes, and fusion across supported styled shared edges remain available. Matched review-correction examples and their native
valence/preserved-bond checks are included in the validation record.

| Before                                                                                                                              | After                                                                                                                                           |
| ----------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| ![Before: regular-ring placement creates carbon valence six and four coincident atom pairs.](images/regular-ring-safety/before.png) | ![After: invalid regular-ring placements are rejected while the original molecular graphs remain intact.](images/regular-ring-safety/after.png) |

![Valid phosphorus attachment and bold double-bond fusion remain available; phosphorus bond valence is three and the original styled edge is retained.](images/regular-ring-safety/valid-attachments-after.png)

## Stable canvas during selection

By @Ameyanagi in [PR #74](https://github.com/Ameyanagi/ReShiki/pull/74), following @HiroYokoyama's proposal in [issue #61](https://github.com/Ameyanagi/ReShiki/issues/61).

Selecting atoms and opening object properties keeps the drawing steady, making double-click molecule selection reliable. Bonded-movement controls stay in the existing scrolling context row. Explicit Fit, inspector toggles, window resizing, pan, and zoom keep their behavior.

Matched examples come from application-renderer and pointer-event checks. Real macOS desktop checks also passed on combined source `446331e`, including the minimum window size, inspector visibility changes, and the optional object toolbar. [Reproduction steps, fixtures, measurements, and exact capture sources](changes/selection-canvas-stability.md).

| Before                                                                                                              | After                                                                                                                         |
| ------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| ![A second click misses the nitrogen after the canvas shifts](images/selection-canvas/before-1280-double-click.png) | ![Double-click selects the whole molecule while the drawing stays fixed](images/selection-canvas/after-1280-double-click.png) |

## Precise numeric transforms

By @Ameyanagi in [PR #73](https://github.com/Ameyanagi/ReShiki/pull/73).

Enter exact rotation, tilt, dimensions, and scale in the selection inspector, with optional proportional resizing and one-step Undo. Fonts and line widths stay fixed. Width and height include labels; sizes that cannot be reached with fixed fonts show an error without changing the drawing.

![A selected group resized independently to 80 pt height while its width stays 101.91 pt](images/pr-reviews/numeric-height80-unlocked.png)

This desktop capture comes from the combined integration build, with other parallel features including the object toolbar. Numeric input, validation, Undo/Redo, and saving were checked; see the [source disclosure and reproduction notes](changes/numeric-transforms.md).

## Reaction and recent-molecule shortcuts

By @Ameyanagi in [PR #72](https://github.com/Ameyanagi/ReShiki/pull/72). Native macOS interaction checks passed in the combined integration build.

Build reaction steps with **Cmd/Ctrl+Shift+Right**, and use **Space** in Select mode to return to the molecule you just edited. The reaction shortcut adds an arrow and selects a product copy in one Undo step, preserving groups, abbreviations and reaction roles. Space follows Undo/Redo; focused text retains its typing and caret keys.

The unmodified native captures below show Cmd+Shift+Right creating a reaction copy, then Space recalling the edited product. They come from combined source `446331ec8ffdef3c852cccec8b13e2105d9e6737`; the visible object toolbar and inspector layout include other PRs under review. Foreground objects occupying the reaction destination must be moved first; background graphics do not block copying. [Behavior, fixtures, capture conditions and validation](changes/reaction-selection-shortcuts.md).

![Native desktop: Cmd+Shift+Right creates an arrow and selected product copy](images/reaction-selection-shortcuts/desktop-reaction-copy.png)

![Native desktop: Space selects the most recently edited product after changing its terminal atom to N](images/reaction-selection-shortcuts/desktop-space-selection.png)

## EMF figure export on Windows

By @Ameyanagi in [PR #71](https://github.com/Ameyanagi/ReShiki/pull/71).

Export a vector EMF picture on Windows for Microsoft Office, preserving outlined labels, publication size and the canvas background. Choose **Export → Figure → EMF**. The file includes the whole drawing; keep the `.rsk` original to edit it later. EMF import remains unsupported, and figures larger than 40 inches in either dimension still require PDF or SVG.

![EMF export preserving ethanol labels, an oxidation caption and a reaction arrow on white paper.](images/emf-file-export/light.png)

[Dark-canvas example, reproduction and validation](changes/emf-file-export.md).

## Optional object toolbar

By @Ameyanagi in [PR #70](https://github.com/Ameyanagi/ReShiki/pull/70).

Show the optional object toolbar for one-click alignment, distribution, reflection, rotation, and graphics/bond stacking. Enable **View → Object toolbar**; ReShiki remembers the preference between sessions. Controls stay in place when the selection changes, with unavailable commands disabled.

Front/back changes graphics and bond depth; text and reaction arrows do not have editable stacking order. Mixed graphics/bond changes undo together. [Reproduction details](changes/object-toolbar.md) include standalone renderer checks and desktop interaction checks on the combined integration build.

![Three molecules aligned by clicking Align top edges in the desktop object toolbar](images/object-toolbar/desktop-align-top.png)

## TBDPS and OTBDPS protecting groups

By @Ameyanagi in [PR #69](https://github.com/Ameyanagi/ReShiki/pull/69).

Enter TBDPS or OTBDPS as real protecting groups, preserving chemistry when expanding, saving, and exchanging drawings. TBDPS attaches through silicon; OTBDPS attaches through oxygen and reverses its label to TBDPSO when the bond is on the right. Both are available from atom-label entry and the chemical-abbreviation presets. Selecting a phenyl ring inside an expanded protecting group also works with **Contract common groups**; an unselected larger group no longer blocks the selected fragment.

![TBDPS and OTBDPS with left and right attachments, expanded structures, and matching molecular formulas.](images/tbdps-abbreviations.png)

The image is application renderer output; [reproduction and molecular checks](changes/tbdps-abbreviations.md) document the full graph and editable exchange.

## Windows GPU rendering and nightly builds

By @Ameyanagi in [PR #68](https://github.com/Ameyanagi/ReShiki/pull/68).

Windows builds include WGPU with a Tiny Skia fallback. CPU-only environments keep
the direct software renderer, and explicit renderer overrides remain available
for testing. The [validation record](windows-gpu-validation.md) includes Windows
VM measurements and real Radeon GPU rendering on Linux; physical Windows GPU
performance remains unmeasured.

Development PRs target `main`. Successful nightly builds publish installers and portable
archives as prereleases for all six platforms, with unique versions and checksums. Stable
releases use version tags on tested commits in `main`.

## Stable downloads and optional nightly builds

By @Ameyanagi in [PR #81](https://github.com/Ameyanagi/ReShiki/pull/81).

The homepage, repository README, and installation guide now explicitly offer the latest stable release for everyday work. A smaller Nightly builds link leads to separate testing instructions. GitHub's all-releases list still includes newer prereleases; the primary download links go directly to stable. [Change and validation notes](changes/release-channels.md).

![ReShiki homepage with an orange Download stable button, a Stable release version badge, and a smaller Nightly builds link beside the installation guide.](images/release-channels/homepage.jpg)

## Website videos and reusable color palettes

By @Ameyanagi in [PR #59](https://github.com/Ameyanagi/ReShiki/pull/59).

The home page now includes the full ReShiki tour, with a separate release-highlights video in the 0.9.1 notes. The README links to both videos.

The new [color palette reference](https://reshiki.com/guide/color-palettes/) compares all 118 elements in Presentation, Pastel and Jmol, in both light and dark mode. Copy exact RGB or derived OKLCH values, or download JSON, CSV, CSS variables and native theme files. A regression test checks the published data against ReShiki’s color code.

The palette panel follows its Light / Dark toggle independently of the website interface. Select an element to inspect its color, or choose paper, ink and ring-fill swatches.

| Light canvas                                                                                                                        | Dark canvas                                                                                                                                   |
| ----------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| ![Presentation palette on white paper, with selenium selected and its RGB and OKLCH values shown.](images/color-palettes/light.png) | ![Presentation palette on black paper, with selenium selected and the surrounding panel matching the canvas.](images/color-palettes/dark.png) |
