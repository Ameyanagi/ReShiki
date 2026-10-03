# Native layout evidence and Help update (#78)

This record updates the layout documentation for production `783b154` on
**2026-10-03**. The native comparison images were captured on `483e70b`; the Help
images are application-renderer output from that same source. Two separate
focused-arrow checkpoints were captured on actual `783b154`. Their exact
identities and the limited source comparison are below. [Issue #78's combined
acceptance](ui-declutter-validation.md) remains open.

## Capture provenance

| Evidence                    | Actual source and scope                                                                                                                                                                     |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Native before               | macOS arm64 Nightly `0.9.1-nightly.20260929.36525615302.1`, commit `ae5ec464ace991ec498bb1acb155635cc7262c1a`                                                                               |
| Native after                | ReShiki 0.10.0, production `483e70bf168430eaad76c57ee9739512bffb0c51`; QA build `e5339283785b90bf5e01e27ba1d95d8b81181fad` has identical application sources and a reviewed CI/test overlay |
| Help renderer               | Real Iced widgets drawn by `ui_layout_snapshots` on `483e70b`, at 1280 × 820 and 1040 × 680 pixels, Light theme and 100% zoom                                                               |
| Native focused-arrow retest | Production `783b154`, source-qualified QA build `eeb3d67ce398b15b522d992d3f086098a359e9f5`, run `37120484130`                                                                               |
| Current source              | `783b154d6232f960146396c1dc7b594924191895`, a direct child of `483e70b`; only `src/app/file_shortcuts.rs` and its focus regression tests changed                                            |

The `783b154` change prevents focused text fields from forwarding unhandled
arrow keys to drawing commands. The other 1,978 tracked entries have identical
modes and blobs, including the layout and Help source. This supports retaining
the static layout evidence; it is not a claim that these images were captured
on `783b154`. The changed keyboard behavior has a separate native retest.

The [asset manifest](../images/ui-declutter/native-2026-10-03/manifest.json)
contains every image's original SHA-256, dimensions, fixture identity, source
receipt and executable hash. All 31 images preserve their original JPEG or PNG
bytes, without cropping, retouching or re-encoding. Historical [renderer
pairs](ui-declutter.md) and feature-specific captures remain available.

The native R1 pairs share macOS 26.5.1 (25F80), Apple M4, an LG HDR 4K display
reported at 3008 × 1692 logical points and 60 Hz, Light appearance, Publication
palette, JACS / ACS style and Arial 10 pt document text. The runtime renderer
adapter and resolved UI fallback font were not instrumented. Native content
sizes are **1280 × 820** and **1040 × 680** points: the original 2560 × 1696 and
2080 × 1416 images include a 56-pixel title bar at 2× backing scale.

Both builds opened identical version-15 fixture bytes. Both native views used
the 500% zoom clamp followed by six minus actions, giving exact `f32` zoom
`1.3107200860977173` (displayed **131%**). The earlier renderer pairs use 100%.
These comparisons match selection and disclosure states; native recentering
and the removed toolbar rows mean that object positions are not pixel-aligned.
Four native savebacks were independently checked against the shared fixtures.

## Native pairs: 1280 × 820

| State                      | Before: native `ae5ec46`                                                                                                                         | After: native `483e70b`                                                                                                                                |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Default, Properties open   | ![Default, Properties open, historical native 1280x820](../images/ui-declutter/native-2026-10-03/before/default-1280x820.jpg)                    | ![Default, Properties open, native 483e70b 1280x820](../images/ui-declutter/native-2026-10-03/after/default-1280x820.jpg)                              |
| Default, Properties closed | ![Default, Properties closed, historical native 1280x820](../images/ui-declutter/native-2026-10-03/before/default-inspector-closed-1280x820.jpg) | ![Default, Properties closed, native 483e70b 1280x820](../images/ui-declutter/native-2026-10-03/after/default-inspector-closed-corrected-1280x820.jpg) |
| Molecule selected          | ![Molecule selected, historical native 1280x820](../images/ui-declutter/native-2026-10-03/before/molecule-1280x820.jpg)                          | ![Molecule selected, native 483e70b 1280x820](../images/ui-declutter/native-2026-10-03/after/molecule-1280x820.jpg)                                    |
| Mixed selection / Arrange  | ![Mixed selection / Arrange, historical native 1280x820](../images/ui-declutter/native-2026-10-03/before/mixed-1280x820.jpg)                     | ![Mixed selection / Arrange, native 483e70b 1280x820](../images/ui-declutter/native-2026-10-03/after/mixed-1280x820.jpg)                               |
| Transform, More closed     | ![Arrange & transform; no More disclosure, historical native 1280x820](../images/ui-declutter/native-2026-10-03/before/transform-1280x820.jpg)   | ![Transform, More closed, native 483e70b 1280x820](../images/ui-declutter/native-2026-10-03/after/transform-more-closed-1280x820.jpg)                  |
| Transform, More open       | ![Arrange & transform; no More disclosure, historical native 1280x820](../images/ui-declutter/native-2026-10-03/before/transform-1280x820.jpg)   | ![Transform, More open, native 483e70b 1280x820](../images/ui-declutter/native-2026-10-03/after/transform-more-open-1280x820.jpg)                      |
| Arc selected               | ![Arc selected, historical native 1280x820](../images/ui-declutter/native-2026-10-03/before/arc-1280x820.jpg)                                    | ![Arc selected, native 483e70b 1280x820](../images/ui-declutter/native-2026-10-03/after/arc-1280x820.jpg)                                              |

## Native pairs: 1040 × 680

| State                      | Before: native `ae5ec46`                                                                                                                         | After: native `483e70b`                                                                                                                      |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Default, Properties open   | ![Default, Properties open, historical native 1040x680](../images/ui-declutter/native-2026-10-03/before/default-1040x680.jpg)                    | ![Default, Properties open, native 483e70b 1040x680](../images/ui-declutter/native-2026-10-03/after/default-1040x680.jpg)                    |
| Default, Properties closed | ![Default, Properties closed, historical native 1040x680](../images/ui-declutter/native-2026-10-03/before/default-inspector-closed-1040x680.jpg) | ![Default, Properties closed, native 483e70b 1040x680](../images/ui-declutter/native-2026-10-03/after/default-inspector-closed-1040x680.jpg) |
| Molecule selected          | ![Molecule selected, historical native 1040x680](../images/ui-declutter/native-2026-10-03/before/molecule-1040x680.jpg)                          | ![Molecule selected, native 483e70b 1040x680](../images/ui-declutter/native-2026-10-03/after/molecule-1040x680.jpg)                          |
| Mixed selection / Arrange  | ![Mixed selection / Arrange, historical native 1040x680](../images/ui-declutter/native-2026-10-03/before/mixed-1040x680.jpg)                     | ![Mixed selection / Arrange, native 483e70b 1040x680](../images/ui-declutter/native-2026-10-03/after/mixed-1040x680.jpg)                     |
| Transform, More closed     | ![Arrange & transform; no More disclosure, historical native 1040x680](../images/ui-declutter/native-2026-10-03/before/transform-1040x680.jpg)   | ![Transform, More closed, native 483e70b 1040x680](../images/ui-declutter/native-2026-10-03/after/transform-more-closed-1040x680.jpg)        |
| Transform, More open       | ![Arrange & transform; no More disclosure, historical native 1040x680](../images/ui-declutter/native-2026-10-03/before/transform-1040x680.jpg)   | ![Transform, More open, native 483e70b 1040x680](../images/ui-declutter/native-2026-10-03/after/transform-more-open-1040x680.jpg)            |
| Arc selected               | ![Arc selected, historical native 1040x680](../images/ui-declutter/native-2026-10-03/before/arc-1040x680.jpg)                                    | ![Arc selected, native 483e70b 1040x680](../images/ui-declutter/native-2026-10-03/after/arc-1040x680.jpg)                                    |

The old inspector had no **More** disclosure, so both new Transform states use
the same historical Arrange & transform view. Its compact numeric fields were
below the initial fold; the [additional original scrolled capture](../images/ui-declutter/native-2026-10-03/before/transform-numeric-visible-1040x680.jpg)
shows them. Ordinary vertical inspector scrolling is retained.

The large Properties-closed image is the accepted replacement capture, with
four tabs open. The historical comparison has one tab. An earlier mislabeled
capture showed Properties open because the screenshot lagged the action; that
image is retained in the QA record and is not presented as the closed state.
Pointer halos visible in the accepted originals have also been preserved.

## Help renderer output

The current Help text describes **Open shortcut examples** as opening a tab,
reusing an unchanged examples tab, and saving a personal copy. The previous
Help illustration showed a separate-window workflow and has been replaced in
[the shortcut guide](shortcut-help.md). These two images show application
widgets rendered offscreen; they are not native desktop screenshots or timing
measurements.

| 1280 × 820                                                                                                          | 1040 × 680                                                                                                          |
| ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| ![Help application-renderer output at 1280 by 820](../images/ui-declutter/native-2026-10-03/renderer/help-1280.png) | ![Help application-renderer output at 1040 by 680](../images/ui-declutter/native-2026-10-03/renderer/help-1040.png) |

## Native focused-arrow retest on `783b154`

These are two original native checkpoints after the numeric-field arrow-key
sequence, from the actual `783b154` source-qualified executable. Rotation
retains its uncommitted `123` draft while the drawing stays unchanged. The
recorded suite tested all four arrows with no modifier, Option and
Shift+Option in both this field and the color popover, at both content sizes:
**48 focused cases**. Five native savebacks matched the baseline bytes. The
separate unfocused Option+Down action still rotated 15°, and Undo restored
the exact baseline.

| 1280 × 820 content, 250% displayed zoom                                                                                                       | 1040 × 680 content, 204% displayed zoom                                                                                                           |
| --------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| ![Native 783b154 Rotation draft retained after arrow keys, large](../images/ui-declutter/native-2026-10-03/r5-783b154/large-arrows-final.jpg) | ![Native 783b154 Rotation draft retained after arrow keys, compact](../images/ui-declutter/native-2026-10-03/r5-783b154/compact-arrows-final.jpg) |

The independent size addendum resolves the earlier outer-window labels:
1280 × 848 and 1040 × 708 points include the 28-point title bar, so their
application content is 1280 × 820 and 1040 × 680. The original images and
receipts remain unchanged. The compact image's enabled Redo belongs to the
preceding intentional rotation/Undo; focused arrow keys did not add history.
The pictures illustrate the checkpoints; the per-key accessibility records
and native savebacks establish the behavior. They do not measure caret
position or latency, or establish Windows/Linux behavior.

## Acceptance scope

The native R1 review accepted all 14 requested layout states and four
savebacks at `483e70b`, with no unresolved product finding in that bounded
comparison. It does not establish keyboard, performance, export or external
application acceptance from pictures alone.

The October 3 reconciliation retains the recorded macOS Arrange visibility
and same-profile restart check. Help focus restoration, point-entry controls
and shared popovers also have targeted native evidence. On actual `783b154`,
48 focused numeric/color arrow-key cases and an unfocused 15° rotation with
Undo passed after the focus guard fix. Other keyboard activation, text-entry
and Escape subcases remain open; this is not a blanket W6 pass.

The current-candidate ChemDraw repeat is recorded separately on macOS with
ChemDraw Prime **26.0.0.6599**. Its bounded result is accepted after independent
review: 24 checks (21 passes and three passes with disclosed exchange limits).
It used a **matched-settings QA destination**: a fresh ChemDraw document took
the source drawing style, was cleared, and then received the
actual clipboard paste. Actual ChemDraw CDX/CDXML saves and eight ReShiki GUI
savebacks were retained. The default-destination style-conflict workflow was
not run successfully and remains a limitation.

The six TBDPS/OTBDPS examples retained the checked chemical identities,
attachments, group labels and effective styles. Seven arcs returned as editable
cubic paths with geometry within the existing format tolerances. External
returns do not preserve ReShiki's native arc start/sweep/depth metadata.
Caption origins shifted by up to about **1.833 pt** for the abbreviations and
**2.086 pt** for the arcs; six formula captions changed line height by about
**0.05 pt**. CDX style numbers have sub-one-wire-quantum rounding. These are
explicit exchange limits, not pixel or exact-floating-point identity. The
report does not cover other ChemDraw versions or Windows.

Remaining acceptance includes the unrecorded compact workflows and preview
sequences, residual keyboard cases, the combined defect disposition, native
Windows review and Windows Word/PowerPoint/Excel editable-object workflows.
Help opening, gallery opening and loaded interactions need their separate
Nightly timing disposition. The original 17 criteria remain in the
[acceptance record](ui-declutter-validation.md); no issue or release is marked
complete by this documentation update.
