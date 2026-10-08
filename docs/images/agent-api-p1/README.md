# Experimental agent API: P1 visual evidence

These are real outputs from the ReShiki CLI and MCP server, captured on
2026-10-08 on macOS 26.5.1 (Apple Silicon). The implementation commit is
`45db305d7d3fbd7d2cf0c635f376b5bf3cbe5bd3`; its stack starts at
`40c8d82a5ed213685a7eedd150a0b7adf6899846` on `main`. The capture used a
locked, default-feature debug build of `target/debug/reshiki`, an empty
`RESHIKI_DATA_DIR`, system fonts, and no app window. There is no GUI zoom
setting for these headless outputs. The API is experimental and is not in
the published 0.11.0 release.

The images demonstrate new headless access to the existing renderer and
layout engine. They do not demonstrate new GUI controls, real Claude/Codex
client interoperability, or completed release acceptance gates. The two
editing images are states before and after an operation at the same head
commit, rather than a bug comparison against the previous PR.

| Image                | Reusable caption                                                  | Capture settings                                                                                                                   |
| -------------------- | ----------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `aspirin.png`        | Render a structure from SMILES without opening the editor.        | CLI preview PNG; white; maximum 1200 × 800; actual 311 × 264. Also identical to the MCP preview before insertion.                  |
| `esterification.png` | Compose a labeled reaction scheme from a structured Proposal.     | CLI publication PNG; default style; 1200 dpi; actual 4773 × 1233. MCP compose/export produced identical PNG bytes on this machine. |
| `mcp-after.png`      | Insert another structure into a session drawing as one undo step. | MCP preview PNG; white; maximum 1200 × 800; actual 311 × 426. Aspirin and an inserted ethanol molecule.                            |

The esterification Proposal is the same example as
`tests/common/headless.rs::esterification`; its complete input is
[`esterification.json`](esterification.json). The aspirin input is the
SMILES `CC(=O)Oc1ccccc1C(=O)O`; the inserted ethanol is `CCO`. No external
image or third-party illustration was used.

## CLI reproduction

Run from the repository root, with a fresh output directory:

```sh
cargo build --locked
mkdir -p /tmp/reshiki-p1-evidence
RESHIKI_DATA_DIR=/tmp/reshiki-p1-evidence/data target/debug/reshiki --cli render --smiles 'CC(=O)Oc1ccccc1C(=O)O' --to png --width 1200 --height 800 -o /tmp/reshiki-p1-evidence/aspirin.png
RESHIKI_DATA_DIR=/tmp/reshiki-p1-evidence/data target/debug/reshiki --cli compose docs/images/agent-api-p1/esterification.json --to png -o /tmp/reshiki-p1-evidence/esterification.png
RESHIKI_DATA_DIR=/tmp/reshiki-p1-evidence/data target/debug/reshiki --cli analyze --smiles 'CC(=O)Oc1ccccc1C(=O)O'
```

`compose` reports `PNG: 4773 × 1233 pixels at 1200 dpi` as a warning on
stderr and exits 0. The original capture used binary stdout redirected to
files; the equivalent `-o` commands above leave stdout empty. An existing
output requires `--force`. Font-dependent pixels can differ on another OS.

## MCP reproduction and observed results

Use `scripts/agent_api_client.py::StdioClient` or another MCP client to
start the same binary with `--mcp`, an empty data directory, and separate
read/write grants for a dedicated output folder. For the 2026-07-28 revision,
the repository client adds the required per-request `_meta`.

1. `import` aspirin with `{"format":"smiles","text":"CC(=O)Oc1ccccc1C(=O)O"}`.
   Retain the returned document handle and revision `"0"`.
2. `inspect` with `{"document":"<aspirin>","ids":null}` returns 13 atoms
   and 13 bonds. `render` with `format: "png"`, `max_width: 1200`,
   `max_height: 800`, and `ids: null` produces `aspirin.png`.
3. `import` ethanol with `{"format":"smiles","text":"CCO"}`. Then call
   `apply` with the following arguments:

   ```json
   {
     "document": "<aspirin>",
     "edit": "insert",
     "source": "<ethanol>",
     "ids": null,
     "base_revision": "0",
     "idempotency_key": "p1-overview-insert"
   }
   ```

4. The returned revision is `"1"`. Inspection now returns 16 atoms and
   15 bonds; rendering with the same settings produces `mcp-after.png`.
   Repeating the exact request with the same key returns the identical
   receipt and does not insert another molecule.
5. Undo with the old `base_revision: "0"` is refused with `stale`. Undo
   with revision `"1"` restores the original object counts; redo with the
   returned revision restores the inserted counts.
6. `file_save` with `format: "reshiki"`, `pages: null`, and
   `overwrite: false` saves the edited drawing as `.rsk` inside the write
   grant. `file_open` with `format: "reshiki"` inside the read grant
   reopens it with the same object counts. A second server without grants
   refuses that read with `access_denied`.
7. `compose` the complete Proposal with `style_document: null`, then
   `export` with `format: "png"` and `pages: null`. The PNG bytes match
   the CLI composition on this machine.

The capture also exercised `info`, `document_new`, `document_list`, and
`document_close`: all 13 advertised tools were called successfully, and
both server sessions exited 0 after stdin closed. A separate launch that
attempted to grant the user's entire home folder exited 2 before reading
stdin. These observations supplement the automated tests; they do not
replace the pending platform and manual acceptance checks.
