---
section: Fixes
issues: [1160, 1183, 431, 430, 88]
---

- Browser editor: an Arial added with `addFont` is now used for labels and export instead of the bundled Liberation Sans stand-in, for each weight and style the added font supplies.
- Python package and `reshiki --cli`: a PNG, SVG or PDF that carries a ReShiki drawing now opens as that drawing (`Drawing.open`, `reshiki.parse`, `convert`, `render`, `analyze`, `name`). A picture without one says "This picture has no ReShiki drawing inside" instead of a UTF-8 or SMILES error. See [Python](python.md) and [the CLI](agent-cli.md#input).
- `<reshiki-editor>`: changing the `src` or `format` attribute loads the new drawing. It first fires a cancelable `beforeload` event with `{ src, format, dirty }`, so a page can keep a drawing with unsaved changes.
- Browser editor: "Opening…" and "Preparing … export…" now show in the status line before opening a file or exporting a figure holds the page.
