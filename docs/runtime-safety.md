# Runtime safety audit — 2026-09-20

This records the worker-based application at the time of the audit. The validation counts and process descriptions below are historical; see [architecture](architecture.md) for the current Rust engine and isolated native InChI helper.

The application and library forbid Rust `unsafe` code. Non-test builds deny
`clippy::unwrap_used`, `expect_used`, `panic`, `unreachable`, `todo`,
`unimplemented`, and `indexing_slicing`. Both crate roots enforce the same rules;
`cargo clippy --all-targets -- -D warnings` checks the production builds as well
as tests. Test assertions intentionally fail when a regression occurs.

Runtime changes include:

- Checked atom/bond/template lookups and slice iteration in editing, placement,
  selection handles, previews, rendering and the chemistry bridge.
- Fallible worker startup, protocol decoding and output serialization; request
  counter exhaustion returns an error and allows worker restart.
- Checked clipboard ID allocation. Invalid fragments, missing references or
  exhausted IDs leave the destination untouched.
- Validated edits roll back before entering history when their document is
  invalid. Template attachment rejects invalid geometry and source graphs.
- Safe signed-charge formatting and saturating charge changes, including
  `i32::MIN` and `i32::MAX` inputs.
- UTF-8-safe color/text/number parsing; bounded sequences and checked numeric
  increments. Missing or malformed bundled templates produce a library notice;
  invalid bundled style JSON falls back to explicit JACS settings.
- SVG string construction no longer unwraps formatting results. Image exports
  validate their document, retain the existing raster size cap, and return
  allocation/encoding errors.

Regression coverage exercises dangling bonds, stale anchors and handles,
invalid geometry, exhausted IDs, extreme charges, Unicode and invalid text
ranges, malformed chemistry requests, and recovery on the following valid
request. It also checks atomic rollback and the bundled JACS/catalog data.

Validation: 125 Rust tests and 36 Python tests passed; formatting and strict
Clippy passed. The current application was exercised through native computer
control for label editing, dragging, Undo/Redo and save/open. Logs are under
`artifacts/atom-labels-qa-20260920/` (local, ignored by Git).

These checks cover ReShiki's runtime source and the exercised inputs. They do
not prove that operating-system services or third-party libraries can never
fail. The chemistry worker remains a separate process, and bridge errors are
reported without replacing the current drawing.

## Native clipboard update

The macOS helper uses checked optionals and explicit errors, without forced unwraps. Requests/responses are bounded, child execution times out after ten seconds, and the Rust process concurrently drains stdout/stderr. The helper validates all representations before clearing the clipboard. Cut deletes only after a successful write; captured document revisions prevent delayed Cut/Paste from changing newer work.

Binary drawing parsing bounds input to 16 MB, nesting to 64 levels, objects to 100,000 and properties to one million. Malformed lengths, duplicate nonzero identifiers, unsupported objects/properties and detected query predicates produce recoverable errors. Raster clipboard bounds are finite and checked before fixed-point encoding. See [clipboard verification and limits](clipboard.md).

This update passes **135 Rust tests**, **42 Python tests**, formatting and strict Clippy. Test totals above record earlier milestones.

## Agent API (P1, experimental)

The experimental agent API (`reshiki --mcp` and `reshiki --cli`; see the
[agent API reference](agent-api.md)) is in Nightly builds only, not in ReShiki
0.11.0. MCP asks servers to validate tool inputs, enforce access controls,
rate-limit calls and sanitize outputs
([MCP tools, Security Considerations](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)).
This section records how P1 does each, what remains possible, and the tests
that pin it. Gate numbers refer to the P1 acceptance gates.

### Budgets

The operation budgets are `Budgets::default()` in
`crates/agent/src/ops/budget.rs`. The MCP connection limits are
`Limits::from_budgets` in `crates/mcp/src/framing.rs`. The `info` tool and
`reshiki --cli info` report the operation budgets.

| Limit                      | Value                                                      | When exceeded                                                    |
| -------------------------- | ---------------------------------------------------------- | ---------------------------------------------------------------- |
| Request line               | 24 MiB (25,165,824 bytes) before the newline               | JSON-RPC -32600 `Request exceeds N bytes`; serving continues     |
| Request ID                 | 256 bytes for a string ID                                  | -32600 without an ID                                             |
| Input text                 | 16 MiB (16,777,216 bytes) per import, file or CLI input    | `budget`                                                         |
| CDX base64                 | 22,369,624 bytes, the base64 length of a 16 MiB CDX        | `budget`                                                         |
| Documents                  | 16 per server                                              | `budget`                                                         |
| Objects                    | 100,000 per document                                       | `budget`                                                         |
| Object IDs                 | 5,000 per request                                          | `budget`                                                         |
| Session weight             | 2,000,000: objects + 1, summed over documents and history  | `budget`, checked before any change                              |
| Pictures                   | 128 MiB of unique picture data per server                  | `budget`                                                         |
| History depth              | 20 undo steps per document                                 | The oldest step is dropped                                       |
| Inspect                    | 5,000 objects per result                                   | `truncated: true`; counts stay complete                          |
| Idempotency                | 32 receipts per document; keys of at most 128 bytes        | The oldest receipt is dropped; a longer key is invalid           |
| Idle documents             | Expire after 60 minutes without use                        | `unknown_document`                                               |
| Concurrency and queue      | 2 operations run; 8 more wait                              | `busy`                                                           |
| Outstanding requests       | 12 (concurrency + queue + 2), holding at most 48 MiB       | The reader stops reading until a request completes               |
| Deadline                   | 120 s per operation, checked cooperatively                 | `timeout` at the next checkpoint                                 |
| Output                     | 16 MiB of raw output per result (about 21.3 MiB in base64) | `budget`                                                         |
| MCP result                 | 48 MiB serialized                                          | `budget`: request a smaller render or fewer objects              |
| Preview render             | 1600 × 1000 by default; sides 1-8192; 16,000,000 pixels    | `invalid_arguments` for a side, `budget` for the pixels          |
| PNG export                 | 16,000,000 pixels                                          | Resolution steps down from the style's dpi until it fits         |
| Error messages             | 500 characters                                             | Cut, so errors never echo large input                            |
| `agent-access.json`        | 64 KiB                                                     | Exit 2 at startup                                                |
| Heap (`--mcp` and `--cli`) | 2 GiB; `RESHIKI_AGENT_HEAP_MB` sets 256-16384 MiB          | stderr `RESHIKI_HEAP_LIMIT <budget> <used> <requested>`, exit 75 |

An invalid `RESHIKI_AGENT_HEAP_MB` is a usage error (exit 2) before anything
is read. The heap ceiling counts the server or command's own allocations; the
InChI and geometry workers it starts keep their own limits.

### Containment

- **Transport.** `reshiki --mcp` serves only the standard input and output
  of the client that started it. It opens no listener and makes no network
  requests. Stdout carries MCP messages only. Stderr carries a bounded log
  that never includes drawing, file or request content; a panic logs its
  location, never its payload.
- **Sessions.** Documents live in the server's memory, behind opaque handles
  checked against their owner. The server cannot reach drawings open in the
  app.
- **Files.** Only `file_open` and `file_save` touch files, and only inside
  folders granted at startup with `--allow-read` and `--allow-write` or in
  `agent-access.json`. Read and write grants are separate. Each folder is
  opened once, from its canonical path, as a directory handle (cap-std).
  Every request must be a plain absolute path inside a granted folder, with
  an allowed extension, and is resolved relative to that handle: `..` and
  symlinks that leave the folder are refused. Writes go
  to a temporary file beside the target, then a no-clobber hard link
  publishes a new file or a rename replaces an existing one when `overwrite`
  is true. Folders are never created.
- **Refused folders.** File system roots, the home folder and its ancestors,
  folders that overlap ReShiki's data folder, earlier installations' data or
  the executable's folder, Unix `/proc`, `/sys` and `/dev`, and the Windows
  system folder cannot be granted, even before they exist. A refused folder
  or an unreadable `agent-access.json` exits 2 before stdin is read.
- **Client roots.** The server never requests MCP roots, which the
  2026-07-28 revision deprecates as informational guidance rather than access
  control. A client's project folder, such as Claude Code's
  `CLAUDE_PROJECT_DIR`, grants nothing.
- **Fences.** Outside its `access` module, `reshiki-agent` cannot call
  filesystem APIs, and `reshiki-mcp` uses none (`clippy.toml` in each crate,
  checked by `tests/test_agent_fs_fence.py`).
- **Command line.** `reshiki --cli` reads and writes the paths on its command
  line with the user's own permissions, without grants or an extension
  allowlist. It never replaces an existing output without `--force`.
- **Windows.** Both modes keep the client's pipes out of the worker processes
  they start, so a worker that outlives the server cannot hold the client's
  output open; startup fails if a pipe stays inheritable.

### Residual risks

- **Hard links.** A file inside a granted folder can be a hard link to a file
  outside it. Reads follow it. Refusing every multiply linked file would
  also refuse ordinary files, so this is accepted. Replacing such a file
  writes a new file and leaves the outside file's bytes unchanged.
- **Same-user processes.** Another process running as the same user can swap
  folders, including while the grants are being opened at startup. The
  post-open checks narrow that window; they cannot close it.
- **Roots captured at startup.** Grants are opened once. A granted folder
  that is later moved keeps resolving inside the folder that was opened, and
  new grants need a restart of the server.
- **Windows identity.** On Unix the opened folder's device and inode must
  match its canonical path. cap-std offers no such check on Windows, so only
  the canonical path is checked again there.
- **Cloud placeholders.** Reading a OneDrive, iCloud Drive or similar
  placeholder may make the sync client download it.
- **macOS privacy permissions.** macOS attributes the server's file access to
  the client app that started it. Folders in Documents, Desktop, Downloads or
  iCloud Drive work only when that client has permission, and macOS may ask
  on the client's behalf.
- **FAT and exFAT.** Without hard links, `file_save` cannot create a new file
  without risking an overwrite and fails with `no_clobber_unsupported`;
  `overwrite: true` still replaces.
- **Spelling.** Paths match a granted folder exactly on Unix and
  ASCII-case-insensitively on Windows. Other spellings, such as another
  Unicode normalization, fail closed as `path_not_granted`.
- **Interrupted writes.** An interrupted publication can leave a
  `.reshiki-<pid>-<n>.tmp` file beside a complete destination.
- **Cooperative deadline.** Blocking chemistry is not pre-empted. Deadlines
  and cancellation take effect at the next checkpoint, a call keeps its
  permit until its work ends, and the input budgets, the existing worker
  timeouts and the heap ceiling bound that work.

### Malformed input and cancellation

transport-2's framing contract (`crates/mcp/src/framing.rs`) answers every
malformed line and keeps serving:

| Input                                                      | Reply                                                    |
| ---------------------------------------------------------- | -------------------------------------------------------- |
| Not UTF-8 or not JSON                                      | -32700 `Parse error`, without an ID                      |
| JSON that is not an object, or an invalid ID               | -32600 `Invalid Request`, without an ID                  |
| `jsonrpc` other than "2.0", or no string `method`          | -32600, with the ID when it is valid                     |
| A line over the limit                                      | -32600 `Request exceeds N bytes`, with the ID when found |
| A duplicate in-flight ID                                   | -32600 `Duplicate request id` with that ID               |
| A client response, or an unknown or malformed notification | Nothing                                                  |
| An unknown method                                          | -32601                                                   |
| Malformed parameters, an unknown tool or a missing `_meta` | -32602                                                   |
| An unsupported protocol revision                           | -32022 with `data.supported`                             |
| A tool task that panicked                                  | -32603; serving continues                                |
| Invalid tool arguments and every other tool failure        | An `isError` result with an operation error code         |

The ID is omitted, never null, when it cannot be read; 2025-11-25 and
2026-07-28 allow that (`id?: RequestId`).

Cancellation is advisory, and completion may race it
([MCP cancellation](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/cancellation)):

- `notifications/cancelled` for an outstanding request reaches the host once.
  The writer drops that request's response, whenever it arrives.
- A call still queued never runs. A running call stops at its next
  checkpoint and keeps its permit until its blocking work ends.
- Every edit is one atomic commit. A cancel that arrives after the commit
  keeps the edit and suppresses the response. Repeating an `apply` with the
  same `idempotency_key` returns the stored receipt instead of editing again.
- `import`, `compose`, `file_open` and `document_new` are not idempotent. A
  cancelled create can leave a document that `document_list` shows and that
  counts toward the document budget until it is closed or expires.
- A cancel for an unknown or already answered request is ignored. While 12
  requests are outstanding the reader stops, so a cancel is read once a slot
  frees.

The server exits when stdin ends and its replies are delivered. A client that
stops reading stdout keeps it alive until the client reads, closes the pipe
or terminates the process.

### Tests

| Gate | Area                             | Tests                                                                                                                                                                                              |
| ---- | -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G3   | Grants and escapes               | `crates/agent/src/access/escape_tests.rs` (`unix.rs`, `windows.rs`), `crates/agent/src/access/tests.rs`, `tests/agent_api_grants.rs`, `tests/test_agent_fs_fence.py`                               |
| G3   | Command-line files               | `tests/agent_api_cli_files.rs`                                                                                                                                                                     |
| G4   | Budgets, queue, deadline, cancel | `crates/agent/src/ops/exec/tests.rs`, `crates/agent/tests/headless.rs`, `crates/mcp/tests/transcripts.rs`, `tests/agent_api_runtime.rs`                                                            |
| G4   | Backpressure and shutdown        | `tests/mcp_shutdown.rs`, `tests/mcp_stdio.rs`                                                                                                                                                      |
| G4   | Heap ceiling                     | `tests/agent_api_runtime.rs` (`the_heap_ceiling_is_validated_and_applied`)                                                                                                                         |
| G4   | Windows pipes                    | `tests/agent_api_windows_pipes.rs`, also run in the release profile on x64 and ARM64                                                                                                               |
| G5   | Malformed input                  | `tests/agent_api_malformed.rs` with `tests/fixtures/agent-api/malformed/`, `crates/mcp/src/framing/tests.rs`, `crates/mcp/tests/transcripts.rs` (`errors.jsonl`), `crates/mcp/tests/panic_hook.rs` |
