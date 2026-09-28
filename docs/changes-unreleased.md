# Unreleased changes

The Windows dropdown fix is recorded in [ReShiki 0.9.1](changes-0.9.1.md). Publisher-style, theme, clipboard and aromatic-fusion updates are recorded in [ReShiki 0.9.0](changes-0.9.md).

## Reaction and recent-molecule shortcuts

By @Ameyanagi in [PR #72](https://github.com/Ameyanagi/ReShiki/pull/72), **under review**. Desktop interaction QA is pending.

Build reaction steps with **Cmd/Ctrl+Shift+Right**, and use **Space** in Select mode to return to the molecule you just edited. The reaction shortcut adds an arrow and selects a product copy in one Undo step, preserving groups, abbreviations and reaction roles. Space follows Undo/Redo; focused text retains its typing and caret keys.

The examples below are application-renderer captures. Foreground objects occupying the reaction destination must be moved first; background graphics do not block copying. [Behavior, fixtures and validation](changes/reaction-selection-shortcuts.md).

![Cmd/Ctrl+Shift+Right creates an arrow and selected product copy](images/reaction-selection-shortcuts/reaction-copy.png)

![Space selects the most recently edited product after changing its terminal atom to N](images/reaction-selection-shortcuts/space-selection.png)

## Website videos and reusable color palettes

By @Ameyanagi in [PR #59](https://github.com/Ameyanagi/ReShiki/pull/59).

The home page now includes the full ReShiki tour, with a separate release-highlights video in the 0.9.1 notes. The README links to both videos.

The new [color palette reference](https://reshiki.com/guide/color-palettes/) compares all 118 elements in Presentation, Pastel and Jmol, in both light and dark mode. Copy exact RGB or derived OKLCH values, or download JSON, CSV, CSS variables and native theme files. A regression test checks the published data against ReShiki’s color code.

The palette panel follows its Light / Dark toggle independently of the website interface. Select an element to inspect its color, or choose paper, ink and ring-fill swatches.

| Light canvas                                                                                                                        | Dark canvas                                                                                                                                   |
| ----------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| ![Presentation palette on white paper, with selenium selected and its RGB and OKLCH values shown.](images/color-palettes/light.png) | ![Presentation palette on black paper, with selenium selected and the surrounding panel matching the canvas.](images/color-palettes/dark.png) |
