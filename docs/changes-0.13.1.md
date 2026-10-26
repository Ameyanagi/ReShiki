# ReShiki 0.13.1

ReShiki 0.13.1 improves atom-label editing, crystal substitutions, 3D model
controls, drawing tools, recovery and feedback on large drawings. It also
corrects PDF shading and browser clipboard behavior. The changes below retain
their contributor credits and links to the original comparisons and limits.

[Release downloads](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.13.1) ·
[Previous release](changes-0.13.md) ·
[Release validation](release-0.13.1-validation.md).

## File compatibility

**Every drawing saved in 0.13.1 uses document version 30. ReShiki 0.13.0 and
earlier cannot open these files**, even when the drawing contains no crystal or
other new feature. Older versions ask you to update. Older drawings still open
in 0.13.1. Keep a copy saved by an older version before saving in 0.13.1 if you
share drawings with someone who still uses it.

Version 30 preserves the source of bonds inferred from crystal atom distances.
Removing a cell must not silently turn those bonds into validated chemical
bonds. Version 29 crystals still open and acquire this context from their live
cells. If a version 29 drawing's cell was already removed, reopen the original
crystal file to recover the missing context.

## Highlights

- Edit atom labels beside their structure, read the complete choices and accept
  chemical suggestions with one click. Typed protecting groups preserve sugar
  stereochemistry, and repeating a label avoids flashing the editor.
- Keep crystal substitutions, cell copies and import notes together. Properties
  distinguishes whole-drawing values from the selected fragment and explains
  when distance-derived crystal bonds cannot support chemical analysis.
- Move a selected 3D model from its frame edge, reach its controls directly,
  keep captions attached while resizing and open models facing their drawings.
- Review recovery drafts before restoring or discarding them. New drawings
  remember the chosen canvas brightness.
- Read clearer import, export and clipboard feedback. Long Structure-input
  operations show elapsed time and can be cancelled without changing the drawing.

## Fixes and comparisons

@Ameyanagi — Updated application dependencies and build runtimes for the
October security advisories, including CVE-2026-111497, in
[PR #586](https://github.com/Ameyanagi/ReShiki/pull/586). GitHub Actions prepares
and verifies a patched runtime before running JavaScript actions. The
[security review](changes/security-dependency-audit.md) records the updates,
remaining scoped findings and verification limits tracked in
[issue #585](https://github.com/Ameyanagi/ReShiki/issues/585).

@Ameyanagi — PDF copies of large selections show progress before
conversion and retain the canvas caches in
[PR #582](https://github.com/Ameyanagi/ReShiki/pull/582).
[Native timing, copied-output comparison and reproduction](changes/large-selection-copy-feedback.md).
Local M1 acknowledgment improves from 616.262 ms to 53.678 ms, with completion
from 1,564.521 ms to 776.795 ms. The literal 33.3 ms/M4 target remains unverified.

![Matched first native PDF progress frames and readable footer close-ups](images/qa-large-selection-copy/native-first-progress.png)

Space-filling models keep hydrogens facing the viewer visible where overlapping
atoms meet. The front cap no longer leaves a hole through its carbon.
@Ameyanagi, [PR #535](https://github.com/Ameyanagi/ReShiki/pull/535).
[Matched renderer review](changes/qa-513.md).

| Before                                                                                 | After                                                                             |
| -------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| ![A front-facing hydrogen leaves a hole through carbon](images/qa-513/before-dark.png) | ![The visible hydrogen cap fills the carbon cutout](images/qa-513/after-dark.png) |

PDF export keeps 3D model and crystal shading aligned at publication size.
@Ameyanagi, [PR #536](https://github.com/Ameyanagi/ReShiki/pull/536).
[Matched export review and reproduction](changes/qa-522.md).

![Matched PDF exports before and after the fix: Flat and Glossy molecular models and a sodium chloride unit cell retain their shading](images/qa-522/pdf-gradient-before-after.png)

Crystal files passed to the CLI now produce a clear unsupported-format message
instead of a SMILES parse error. Rejection happens before input is read or output
is created. @Ameyanagi, [PR #537](https://github.com/Ameyanagi/ReShiki/pull/537). [Command examples and validation](changes/qa-490.md).

@Ameyanagi — long polystyrene and alanine chains import with
separated rings and chemical labels while retaining molecular identity and
stereochemistry in [PR #539](https://github.com/Ameyanagi/ReShiki/pull/539).
[Matched renderer comparisons, inputs, and bounded fallback behavior](changes/long-chain-layout.md).
Import latency is tracked separately in [#508](https://github.com/Ameyanagi/ReShiki/issues/508).

![Poly-L-alanine with separated labels and retained stereochemical wedges](images/long-chain-layout/stereo-alanine200-after-detail.png)

@Ameyanagi — drag the visible frame edge to move a selected 3D
model while keeping its view and selection in
[PR #540](https://github.com/Ameyanagi/ReShiki/pull/540).
[Matched native desktop evidence and editable fixture](changes/qa-514.md).

![Methanol moves while retaining its selected frame and view](images/qa-514/after-card.png)

@Ameyanagi in [PR #541](https://github.com/Ameyanagi/ReShiki/pull/541).

Safari pastes clipboard text into captions and Import fields without turning Command-V into a typed v.

[Evidence, comparisons, and reusable release material](changes/qa-521.md).

@Ameyanagi in [PR #542](https://github.com/Ameyanagi/ReShiki/pull/542).

Crystal import notes stay with the drawing and survive native save/reopen; the guide now explains the opening view correctly.

[Evidence, comparisons, and reusable release material](changes/qa-crystal-metadata.md).

@Ameyanagi in [PR #543](https://github.com/Ameyanagi/ReShiki/pull/543).

Edit atom labels beside their structure, read every label type in full, and accept catalog suggestions with one click.

[Matched examples and release material](changes/qa-atom-label-editor.md).

@Ameyanagi in [PR #544](https://github.com/Ameyanagi/ReShiki/pull/544).

Fragment-name fields start fresh with a new selection or drawing, saved abbreviations show a short confirmation, and content-specific panels return to Properties when the drawing changes.

[Matched examples and release material](changes/drawing-panel-state.md).

@Ameyanagi in [PR #545](https://github.com/Ameyanagi/ReShiki/pull/545).

Chemical marks and aimed arrowheads stay clear of drawing ink, reaction separators follow visible labels, long terminal text keeps its bond visible, and chemical symbol tools support repeated placement.

[Matched examples and release material](changes/qa-drawing-geometry-2026-10-25.md).

In [PR #557](https://github.com/Ameyanagi/ReShiki/pull/557):
Export dialogs name their format, browser progress and download receipts describe what happened, and the npm package has a complete HTML recipe with the correct release version.
See the [QA report](changes/qa-browser-polish.md) for validation and screenshots.

Contributor: @Ameyanagi.

In [PR #558](https://github.com/Ameyanagi/ReShiki/pull/558):
Crystal substitutions keep their cell and copies, update the drawn formula, and preserve unvalidated bond context after unsupported edits.
See the [QA report](changes/qa-crystal-substitution.md) for validation and screenshots.

Contributor: @Ameyanagi.

In [PR #559](https://github.com/Ameyanagi/ReShiki/pull/559):
Large peptide imports avoid repeated ring searches during label placement. Long-running Structure-input operations show elapsed time and can be cancelled without changing the drawing.
See the [QA report](changes/qa-508-import-performance.md) for measurements and screenshots.

Contributor: @Ameyanagi.

In [PR #560](https://github.com/Ameyanagi/ReShiki/pull/560):
See the label the Text tool will repeat, keep its chemical meaning, and read clear feedback for generic and refreshed drawings.
See the [QA report](changes/qa-label-feedback.md) for validation and screenshots.

Contributor: @Ameyanagi.

In [PR #561](https://github.com/Ameyanagi/ReShiki/pull/561):
Reach 3D controls in one click, bring clipped models into view, recover directly from blocked scale commands, and cancel long native model generation with Escape.
See the [QA report](changes/qa-model-controls.md) for validation and screenshots.

Contributor: @Ameyanagi.

In [PR #562](https://github.com/Ameyanagi/ReShiki/pull/562):
Help prefers shortcuts available in the current drawing mode among equally strong
matches, shows labelled ring-fusion examples, and uses a readable Shift search
hint. See the [QA report](changes/qa-help-polish.md) for validation and screenshots.

Contributor: @Ameyanagi.

Review recovery drafts by title, date and size, restore only the
drawings you need, or move abandoned copies to the Trash. The browser asks before
permanently deleting recovery copies and keeps failed operations retryable.
Contributed by @Ameyanagi in [PR #563](https://github.com/Ameyanagi/ReShiki/pull/563)
for [issue #491](https://github.com/Ameyanagi/ReShiki/issues/491).
[QA report and limits](changes/recovery-manager.md): native Trash interaction was
checked on macOS; Windows and Linux runtime checks remain pending.

![Recovery picker with benzene checked, drawing titles, dates, atom counts and Restore selected or Restore all actions](images/recovery-manager/native-picker.png)

Keep whole-drawing formula and mass visible beside selected-fragment properties,
preserve the explicit Molecular header choice in new and replacement tabs, keep
molecular values visible while choosing drawing tools, and correct singular
atom/bond counts — @Ameyanagi in [PR #564](https://github.com/Ameyanagi/ReShiki/pull/564).
This addresses [#548](https://github.com/Ameyanagi/ReShiki/issues/548),
[#550](https://github.com/Ameyanagi/ReShiki/issues/550),
[#551](https://github.com/Ameyanagi/ReShiki/issues/551), and
[#554](https://github.com/Ameyanagi/ReShiki/issues/554).
[Matched renderer and native input evidence](changes/qa-property-continuity.md).

![After the terminal-bond shortcut, Molecular Properties shows whole styrene C8H8 at 104.152 g/mol beside the selected C2H4 fragment at 28.054 g/mol](images/qa-property-continuity/head-styrene.png)

**Copy as names unsupported text labels, points to ordinary Copy and exposes the
full refusal behind a visible status cue; narrow windows keep every footer
control visible.** @Ameyanagi in
[PR #565](https://github.com/Ameyanagi/ReShiki/pull/565), addressing
[#499](https://github.com/Ameyanagi/ReShiki/issues/499).

CDXML/CDX refusals identify the actual unsupported label instead of calling the
drawing invalid or listing absent ring curves. They suggest the platform's
ordinary Copy shortcut and native save. Strict export still refuses unsupported
content, and a failed Copy as operation leaves the clipboard unchanged. The
footer reserves its height before delayed progress appears, with a second row
below 1280 logical pixels wide.

![Native Copy as refusal names MeO and exposes the complete Command-C, native-save and unchanged-clipboard advice](images/qa-export-feedback/after-full-message.jpg)

[Matched native images, input fixture and combined footer checks](changes/qa-export-feedback.md)
record the macOS menu/hover interaction and real-renderer layout results.
Windows/Linux native interaction and a new ChemDraw round trip are not claimed.

Inspector page changes keep the drawing in place, and positioning
atom marks keeps its controls visible and is available from the atom's context
menu. @Ameyanagi in [PR #566](https://github.com/Ameyanagi/ReShiki/pull/566), for
[#533](https://github.com/Ameyanagi/ReShiki/issues/533) and
[#534](https://github.com/Ameyanagi/ReShiki/issues/534).
[Matched renderer and native QA](changes/qa-inspector-motion.md).

![Inspector page changes keep the drawing in place](images/qa-inspector-motion/page-after.png)

![Positioning atom marks keeps its controls visible](images/qa-inspector-motion/marks-after.png)

@Ameyanagi in [PR #567](https://github.com/Ameyanagi/ReShiki/pull/567):

- Rotation frames follow upright labels, and the angle readout moves clear of label ink while dragging.
- Rotation hints yield when editing an angle, keeping the canvas clear while typing.
- Three atom columns keep all eleven choices visible in a 1600 × 1000 macOS window, with H and C first. The default 1280 × 820 client size still scrolls for lower rows.
- Reopening Structure selects the retained draft for replacement, and Clear empties it.

[Matched before/after images and native interaction verification](changes/qa-drawing-polish.md)
cover all four changes.

![Rotation frame and readout clear of upright NH₂ at 30°](images/drawing-polish/after/rotation-held-30.png)

![Rotation field being edited without its hover hint](images/drawing-polish/after/rotation-context-editing.png)

![All eleven atom choices visible in three columns](images/drawing-polish/after/palette-1600.png)

![Retained Structure draft selected for replacement, with Clear inside the box](images/drawing-polish/after/structure-reopen-opened.png)

In [PR #568](https://github.com/Ameyanagi/ReShiki/pull/568), by
@Ameyanagi: typed protecting groups retain sugar stereochemistry, choose a clear
reading beside nearby labels and repeat without an editor flash. The
[QA record](changes/qa-stereo-repeat.md) includes stereo round trips, matched
Properties results, native interactions and original gesture clips with capture
timestamps. Placement checks use a bounded local context; oversized or
incomplete context retains the established reading.

![Four clear acetate labels after repeating onto the remaining oxygen](images/qa-stereo-repeat/after-polyol-result.png)

- Arrow Properties identifies the selected arrow count in mixed or multiple
  selections, and Atom labels & numbering uses the matching **‹ Properties**
  backlink. @Ameyanagi in [PR #577](https://github.com/Ameyanagi/ReShiki/pull/577)
  for [#575](https://github.com/Ameyanagi/ReShiki/issues/575) and
  [#576](https://github.com/Ameyanagi/ReShiki/issues/576);
  [native review](changes/qa-properties-clarity.md).

[PR #579](https://github.com/Ameyanagi/ReShiki/pull/579) aligns
opening and reset 3D model faces with their drawings, keeps chemical-name
captions attached during model resizing, and shows rotation instructions before
the first drag. Proper rotations preserve chemical stereochemistry; saved views
and crystal cell orientation retain their existing behavior.

![Caffeine opens with the model facing the same direction as its drawing](images/qa-model-face-handles/head-caffeine-both.png)

![The actual browser editor keeps a caption attached during a held model resize](images/qa-model-face-handles/webkit-resize-held.png)

[Matched comparisons, inputs and native/browser checks](changes/qa-model-face-handles.md)
cover the visible changes and the chemical/native round trips. Contributed by @Ameyanagi.

@Ameyanagi — ReShiki remembers the chosen canvas brightness for new
drawings and the next launch, while keeping each saved drawing's own appearance
([PR #581](https://github.com/Ameyanagi/ReShiki/pull/581),
[restart and browser reload evidence](changes/qa-remember-appearance.md)). The
independent **View → Interface** preference remains available.

![A new blank drawing starts clean with the remembered dark canvas and matching interface](images/qa-remember-appearance/after-restart.png)

Artemisinin import and cleanup place the peroxide-adjacent methyl outside the
cage and keep the peroxide bond visible, while preserving all seven
stereocentres. Whole-molecule cleanup refusals now say that the drawing is
unchanged instead of asking for an already complete selection.
@Ameyanagi, [PR #538](https://github.com/Ameyanagi/ReShiki/pull/538).
[Matched renderer review](changes/artemisinin-depiction.md).

| Before                                                                                          | After                                                                                                                            |
| ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| ![Artemisinin with overlapping peroxide oxygen labels](images/artemisinin-depiction/before.png) | ![Artemisinin with an outward methyl, separated oxygen labels and visible peroxide bond](images/artemisinin-depiction/after.png) |

[@Ameyanagi](https://github.com/Ameyanagi) in
[PR #578](https://github.com/Ameyanagi/ReShiki/pull/578):

- Keep a pointer-owned keyboard hotspot under the pointer after wheel pan or
  zoom. Arrow navigation and edits retain their chosen target until actual
  pointer motion or a click takes over.
- Speed up hover queries on large drawings while preserving label, atom and
  bond hit results. On the exact 7,728-atom gallery, optimized M1 CPU work from
  real widget pointer input through app update and retained hover drawing falls
  from 213.3 ms to 20.0 ms. Three matched native phases keep the hotspot within
  one bond of the physical pointer; the QA note records host, camera and
  viewport limits.

| After wheel zoom                                                                                                | During wheel pan                                                                                          |
| --------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| ![A hover-owned hotspot resolves under the pointer after zoom](images/pointer-hover/after/after-wheel-zoom.png) | ![A hover-owned hotspot stays at the pointer during pan](images/pointer-hover/after/during-wheel-pan.png) |

The [source-pinned QA note](changes/qa-pointer-hover.md) includes matched
before/after captures, exact fixture and source hashes, quiet benchmark scope,
and the distinction between pointer and keyboard ownership.

## Measurement limits

The hover CPU comparison includes real widget pointer input, app update and
retained hover drawing. Its 213.3 → 20.0 ms result excludes returned task
execution, OS delivery, GPU rasterization and presentation. Three predeclared
native video phases measured pointer-to-hotspot gaps of 215.63, 121.16 and
199.51 pt before, and 14.08, 7.16 and 13.53 pt after, against a 43.68 pt bond
length with ±2 pt measurement uncertainty. These are representative spatial
measurements on the recorded M1 host, not an all-frame maximum, event-to-paint
latency or application FPS. [Full conditions and receipts](changes/qa-pointer-hover.md).

The PDF copy comparison measures accepted command to presentation submission,
excluding OS input delivery and GPU scanout. It is one quiet warm M1 copy per
build, with different window/framing conditions from the M4 report. Its
53.678 ms first-progress result does not verify the literal 33.3 ms/M4 target.
Resolved PDF object graphs and rendered output match; raw serialization can
vary even between baseline copies. [Full conditions and copied-output checks](changes/large-selection-copy-feedback.md).

## Remaining limits

- The browser editor remains a preview. Printing, the assistant and Office
  integration need the desktop app. Long operations that run on the browser
  thread can still pause the page.
- CDXML/CDX export continues to refuse content it cannot preserve. Clearer
  advice does not remove those format limits or establish a new ChemDraw
  application round trip.
- Recovery Trash interaction was checked on macOS. Windows and Linux graphical
  recovery checks remain unperformed in that review.
- Crystal bonds inferred from distances are not proof of chemical valence.
  Read the [crystal import limits](crystal-structures.md#limits) before using a
  drawing as chemical input.
- Windows and Linux release downloads remain unsigned. macOS publication
  requires signed, notarized and stapled packages; release validation is
  recorded separately and is not implied by the individual fix reviews.

Version, release notes and compatibility guidance: @Ameyanagi in
[PR #583](https://github.com/Ameyanagi/ReShiki/pull/583).

Thank you to all the [contributors](contributors.md) who helped improve ReShiki.
