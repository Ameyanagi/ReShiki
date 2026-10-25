# Unreleased changes

## Under review

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

See [ReShiki 0.13.0](changes-0.13.md) for the latest release notes.
