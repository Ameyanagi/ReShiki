# Unreleased changes

On Linux, **Copy** (**Ctrl+C**) also puts the selection's SMILES on the
clipboard as plain text, the same text **Copy SMILES** gives, so a plain-text
box gets the SMILES. LibreOffice, Chromium pages and ReShiki still paste the
drawing or picture, and when ReShiki quits with its window focused, GNOME still
keeps the picture. A reaction, or a selection without molecules, gets no text.
Windows and macOS are unchanged, because Excel and PowerPoint there would paste
the text instead of the drawing.
[PR #1161](https://github.com/Ameyanagi/ReShiki/pull/1161) ·
[Clipboard compatibility](clipboard-compatibility.md).

SVG files look the same in every viewer. **Export → Figure → SVG** and the
agent tools `export`, `file_save` and `render` now draw text as outlines, as
`reshiki --cli` and the Python package already do. Live `<text>` used the
viewer's own fonts, so labels shifted between apps, and a renderer without
Arial could drop them. To keep text editable, turn on **Keep text editable**
below the export button (off by default; ReShiki remembers it) or pass
`svg_text: "live"` to the tools. An editable drawing inside the SVG still
reopens in ReShiki. **SVG for Office** is unchanged.
[Text in SVG files](figure-export.md#text-in-svg-files).

See [ReShiki 0.14.0](changes-0.14.md) for the prepared release notes and
[release validation](release-0.14.0-validation.md) for publication status.
