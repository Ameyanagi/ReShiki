# Start an empty drawing

Available in Nightly builds; not in ReShiki 0.16.0.

An empty drawing shows a small start block in the center of the canvas.

- Choose **Open…**, **Paste**, or **Templates** to use the existing file,
  clipboard, or template workflows. You can still draw outside the block.
- Choose **×** (**Start with a blank page**) to dismiss the
  block and draw on a completely blank canvas. It stays dismissed for that
  drawing, including when you switch tabs or remove its last object. A new
  drawing shows the start options again.
- Choose a recent file to open it. Desktop builds remember the five files
  opened or saved most recently. Each row shows its folder or GitHub repository.
- Drag a file onto the canvas to see **Drop to open**. The existing file checks
  and import behavior still apply.

Unless dismissed, the block disappears at the first object and returns when
the drawing is empty again. It does not change the drawing, selection, or
Undo history. Use **Import** to insert a chemical name or SMILES.

Recent-file previews use a separate, capped read. Large or picture-heavy files
show a file icon; opening them still uses the ordinary full file reader.
Missing files report their normal open error. ReShiki stores file paths, not
copies of the drawings, in `recent-files.json` in its data folder.

The browser lists available file tabs from the current session. It identifies
them by tab and drawing, so two files with the same name stay separate. Closed
tabs and previous browser sessions require **Open…** again.
