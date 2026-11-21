# Start an empty drawing

Available in Nightly builds; not in ReShiki 0.16.0.

An empty drawing shows a small start block in the center of the canvas.

- Type a chemical name or SMILES and press **Enter** to insert it. Names use
  the same parser as **Import**. Editing the field cancels its previous name
  request. Unsupported names offer **Open Import** for details and alternatives.
- Choose **Open…**, **Paste**, or **Templates** to use the existing file,
  clipboard, or template workflows. You can still draw outside the block.
- Choose a recent file to open it. Desktop builds remember the five files
  opened or saved most recently. Each row shows its folder or GitHub repository.
- Drag a file onto the canvas to see **Drop to open**. The existing file checks
  and import behavior still apply.

The block disappears at the first object and returns when the drawing is
empty again. It does not change the drawing, selection, or Undo history.
Undo removes a structure inserted from the field in one step.

Recent-file previews use a separate, capped read. Large or picture-heavy files
show a file icon; opening them still uses the ordinary full file reader.
Missing files report their normal open error. ReShiki stores file paths, not
copies of the drawings, in `recent-files.json` in its data folder.

The browser lists available file tabs from the current session. It identifies
them by tab and drawing, so two files with the same name stay separate. Closed
tabs and previous browser sessions require **Open…** again.

The start field accepts up to 8 KiB. Use **Import** for larger structure text.
