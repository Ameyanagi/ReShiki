---
section: Fixes
issues: []
---

- **Files that are not regular files.** Opening or inserting a named pipe or
  a folder in the app now fails at once with `not a regular file`. A pipe
  with no writer kept the app waiting on it. `reshiki --cli` refuses such an
  input the same way; pipe it to standard input with `-`.
- **Structure files over 16 MB.** Inserting a MOL, RXN, CDXML, CDX or SMILES
  file over 16 MB now fails with `File exceeds 16 MB` after reading 16 MB of
  it. The whole file was read into memory first.
- **Agent tools: an edit over a budget stops early.** `apply` now refuses an
  edit over the object or session weight limit before it reconciles the
  drawing. Doubling a 59,900-object drawing was refused after 38.7 s in a
  development build and is now refused after 3.9 s.
- **Agent tools: `inspect` on large drawings.** Its time now grows with the
  size of the drawing. A drawing of 45,000 bonded atoms took 30.7 s in a
  development build and takes 1.0 s.
- **Agent tools: an InChI that cannot be made.** `export` and `file_save` to
  InChI fail with `InChI has no identifier for this drawing` instead of
  returning an empty file, for example for a drawing with a wildcard atom.
- **LibreOffice and Microsoft 365 previews.** The preview refuses a drawing
  of more than 100,000 objects before it renders it, and runs under a 2 GiB
  memory limit. A 200,000-atom drawing was refused after 204 s and 1.3 GB in
  a development build; it is now refused after 2.7 s and 0.3 GB.
- **Updates.** ReShiki checks the downloaded update again when you restart to
  install it, and installs nothing if any file of it changed. On Linux an
  update archive that holds a symbolic or hard link is refused. On Windows a
  signed ReShiki installs only an installer signed by the same publisher,
  and a ReShiki whose own signature is not valid installs no update.
- **Linux without `XDG_RUNTIME_DIR`.** ReShiki no longer uses a folder under
  `/tmp` to find its running window. Another user could create that folder
  first, and saving then failed. It uses `/run/user/<uid>`, or your cache
  folder.
- **LibreOffice extension.** Without a chosen app the extension starts the
  installed ReShiki before a `reshiki` found on `PATH`.
- **Microsoft 365 companion.** The companion serves the task pane only to
  the address in the manifests that `setup.js` generates, which now carries a
  private launch key, and gives its token once per served page. Another
  process on the computer could ask for the token before. Run `setup.js`
  with a new `--directory` and sideload its manifests again.
- **Assistant: Accept all edits.** A draft that replaces objects outside your
  selection now waits for **Apply**; see [the assistant](assistant.md#applying-and-undoing).
- **Assistant: other tools.** A request stops with
  `Codex used a tool that ReShiki does not allow` if Codex runs anything but
  the drawing tools, such as a shell command. The assistant no longer writes
  an unreadable reply to the error log.
- **Browser editor in a frame.** `frame.html` answers only the page that
  embeds it, and loads fonts only from that page's origin or its own. A font
  from another origin is reported through `onError` and not loaded.
