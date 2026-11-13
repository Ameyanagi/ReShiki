# Unreleased changes

SVG files look the same in every viewer. **Export → Figure → SVG** and the
agent tools `export`, `file_save` and `render` now draw text as outlines, as
`reshiki --cli` and the Python package already do. Live `<text>` used the
viewer's own fonts, so labels shifted between apps, and a renderer without
Arial could drop them. To keep text editable, turn on **Keep text editable**
below the export button (off by default; ReShiki remembers it) or pass
`svg_text: "live"` to the tools. An editable drawing inside the SVG still
reopens in ReShiki. **SVG for Office** is unchanged.
[Text in SVG files](figure-export.md#text-in-svg-files).

The SVG that ReShiki puts on the clipboard draws its text as outlines on
macOS, Linux and in the browser editor, as Copy Image on Windows already did.
**Copy**, **Copy Image** and **Copy as → SVG** used live `<text>` there, so an
app that pastes the SVG flavor substituted its own fonts. The SVG keeps its
physical size, transparent background and the editable drawing inside it.
[Clipboard](clipboard.md).

See [ReShiki 0.14.0](changes-0.14.md) for the prepared release notes and
[release validation](release-0.14.0-validation.md) for publication status.
