# Unreleased changes

On Linux, **Copy** (**Ctrl+C**) also puts the selection's SMILES on the
clipboard as plain text, the same text **Copy SMILES** gives, so a plain-text
box gets the SMILES. LibreOffice, Chromium pages and ReShiki still paste the
drawing or picture. A reaction, or a selection without molecules, gets no text.
On GNOME, once ReShiki quits, the desktop keeps only that text, so paste before
quitting, or use **Copy Image** for a picture. Windows and macOS are unchanged,
because Excel and PowerPoint there would paste the text instead of the drawing.
[PR #1161](https://github.com/Ameyanagi/ReShiki/pull/1161) ·
[Clipboard compatibility](clipboard-compatibility.md).

See [ReShiki 0.14.0](changes-0.14.md) for the prepared release notes and
[release validation](release-0.14.0-validation.md) for publication status.
