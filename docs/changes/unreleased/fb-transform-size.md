---
section: Fixes
issues: []
---

- **Properties → Transform** no longer refuses a width or height with "This size
  cannot be reached while keeping text and line widths fixed". When the size
  depends only on text, arrowheads or line widths, the drawing stays as it is
  and the status bar says why and what the size remains; a size below what the
  text allows applies the closest size and says so ([Selection transforms](selection-transforms.md)).
