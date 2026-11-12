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

See [ReShiki 0.14.0](changes-0.14.md) for the prepared release notes and
[release validation](release-0.14.0-validation.md) for publication status.
