---
section: Changes
issues: []
---

- **Agent tools: one result shape (breaking).** Every MCP tool now returns
  `{value, warnings, validation, versions}`. `info`, `document_new`,
  `document_list`, `document_close` and `inspect` used to return their fields
  next to `versions`; read them under `value` now, so a new document's handle
  is `value.document` after `document_new` as it is after `import`.
  `compose` reports `bounds` as `{min, max}`, as `inspect` does, instead of
  `[min, max]`. `operation_api` is 2.
  [Changes in operation API 2](agent-api.md#changes-in-operation-api-2) lists
  what a client has to change.
- **Agent tools: errors you can act on.** A tool error is
  `{code, message, retryable}`, with `reason` for file errors (such as
  `path_not_granted`), `param` for the argument at fault and
  `current_revision` after `stale`, so a client no longer reads them out of
  the message. After `stale`, inspect the document again before you retry:
  `current_revision` only says which revision the document has now. On the command line an operation error is
  `reshiki: error[code]: message`; it was `reshiki: error: message`. In Python,
  errors are `reshiki.ReShikiError` (a `ValueError`) with `.code`, and in
  JavaScript every rejected `Error` has `.code`. In Python an unknown input
  format and a constructor given another kind of content, such as a reaction
  for `Molecule.from_smiles`, are `invalid_arguments`.
- **Agent tools: chemical names.** `import` reads a chemical name with
  `format: "name"`, and the new `name` tool returns a drawing's systematic
  name with its status, reason, aliases and alternatives. Both work offline.
  A name's axial configuration is kept, so `(1M)-` and
  `(1P)-[1,1'-binaphthalene]-2,2'-diol` import as two structures, and the
  name parser's warnings and those about the named drawing are in `warnings`.
  On the command line: `echo caffeine | reshiki --cli convert - --from name --to smiles`.
  The server's instructions used to tell agents to write SMILES themselves.
- **Agent tools: `convert`.** One call reads structure text and returns a file
  or a PNG preview, and keeps no document. A picture of a SMILES took
  `import`, `render` and `document_close`.
- **Agent tools: a smaller tool list.** The tool list every MCP client loads
  is 19,418 characters for 16 tools; it was 24,409 for 13. `compose` no
  longer lists the fields of a sketch diagram: the new `schema` tool returns
  them when an agent draws one. A sketch that was valid is still valid.
- **The same names everywhere.** `rsk` and `reshiki` both name a ReShiki
  drawing in MCP, the command line, Python and JavaScript. `cdx`, `rxn` and
  `rsmi` (reaction SMILES) can be written by `export`, `file_save`,
  `reshiki --cli convert` and Python (`cdx()`, `rxn()`, `rsmi()`), as
  JavaScript already could. The option for SVG text is `svg_text`
  (`svgText` in JavaScript) on every surface, and text is outlined unless it
  is `live`.
- **JavaScript package (breaking).** `convert(…, "svg")` and `renderSvg`
  now outline text by default, as the other surfaces do; they kept live text.
  Pass `{ svgText: "live" }` for the earlier output.
- **Python package.** `export()` is the new name of `render()`, which stays.
  `save(path, overwrite=False)` raises `FileExistsError` instead of replacing
  a file; the default still replaces. `svg_text=` is the option's name and
  `text=` still works. `properties()` rounds mass, exact mass, logP and TPSA
  to 9 decimal places, as the command line and MCP do. `export`, `render` and
  `convert` have typed overloads: text formats return `str`, binary ones
  `bytes`.
- **Developer.** The PNG preview and the rounding of properties are in
  `reshiki-io` (`preview::png`, `Analysis::rounded`), shared by the
  assistant, MCP, the command line, the browser library and Python.
  `tests/fixtures/agent-api/parity.json` holds one molecule and one reaction
  with the answers the command line, Python and JavaScript must all give;
  each test suite checks it.
