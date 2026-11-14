---
section: Changes
issues: []
---

- **Agent tools: arguments you do not use.** A tool call may leave out an
  argument that can be `null`, such as `max_width` for `render`; ReShiki reads
  it as `null`. Earlier versions failed with ``missing field `max_width` ``.
  The published schemas still list every argument as required.
- **Agent tools: granted folders in the instructions.** The MCP server's
  instructions name the folders `file_open` reads and `file_save` writes, or
  say `No folders are granted: use import and export.`
- **Agent tools: a smaller tool list.** The tool list every MCP client loads
  is 24,409 characters instead of 27,316. `compose` no longer lists
  `explanation` and `replace_ids`, which it still reads; repeated parts of the
  Proposal schema are stated once; and each parameter's notes are in its own
  description.
- **`reshiki --cli name --json`.** The JSON line also has `warnings` and
  `versions`, as the JSON of the other commands does. The
  [command-line reference](agent-cli.md#name) now covers
  `--isotope-style` and says that charge-balanced salts are named.
