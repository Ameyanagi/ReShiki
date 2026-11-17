---
section: Changes
issues: []
---

- On macOS and Linux, ReShiki keeps its data in `~/.reshiki/`: settings,
  templates, recovery drafts and GitHub working copies. The first launch moves
  the old folder (`~/Library/Application Support/dev.reshiki.ReShiki/` on
  macOS, `~/.local/share/reshiki/` on Linux) there automatically, in one step;
  if it cannot, ReShiki keeps using the old folder and says why in the status
  bar. Windows keeps `%LOCALAPPDATA%\reshiki\ReShiki\data\`.
- **About ReShiki** shows the data folder, with **Open**.
- A GitHub tab's right-click menu has **Show in Finder** (**Show in Explorer**
  on Windows, **Open containing folder** on Linux) for the drawing's working
  copy; so do the command palette and the read-only status chip's menu.
- [Install](https://reshiki.com/guide/install/#uninstall) now says how to
  uninstall completely on each system.
