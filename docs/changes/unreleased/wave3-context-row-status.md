---
section: Changes
issues: []
---

- In [keyboard drawing](3d-keyboard-drawing.md), the context row shows **Mark**
  only when an atom is the target, and **Connect** and **Coordinate** only once
  an atom is marked. Their keys **[**, **]** and **}** appear as keycaps beside
  the names.
- A short context row now folds its last commands into **⋯** first. Selection
  commands such as **Join** and **Group** come first. **Keyboard drawing** and
  **3D optimize…** come last.
- Context row tooltips show the command name and its shortcut, such as
  "Join selected atoms or bonds · Ctrl+J". Screen readers read the same name.
- **Check drawing** findings stay as a count on the Check drawing button until
  the drawing changes. Click the button to open the list. Clicking a finding
  selects its atoms, blinks them on the canvas and moves the view to them if
  needed. See [stereo checks](bond-tools.md).
- The status bar stays on one row at every window width. A long message
  shortens, and its full text is in the tooltip on **…**.
- Hints and tooltips name the Alt key as ⌥ on macOS and Alt on Windows and
  Linux, instead of "Option/Alt".
- The chain tool buttons' tooltips show only the tool name and its key. How to
  use a tool stays in the tooltip on the tool name in the context row.
