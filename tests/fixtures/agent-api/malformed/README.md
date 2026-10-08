# Malformed-input corpus

tests/agent_api_malformed.rs sends every case in `cases.json` to the real
`reshiki --mcp` binary (gate G5): malformed input never crashes the binary or
corrupts stdout, and the server keeps answering afterwards. The same file runs
a separate corpus of malformed `reshiki --cli` command lines and inputs.

The expected values are a contract with the transport (framing and protocol)
and the operation layer (tool error codes). A mismatch is a contract bug to
resolve with the owning area, never a reason to loosen an assertion. When the
framing table in crates/mcp/src/framing.rs changes, this corpus changes in the
same PR.

## Manifest

`cases.json` is an array of cases. Each case has:

- `name`: unique.
- `phase`: how it runs.
  - `modern`: all modern cases share one process. After each case,
    `server/discover` with the full 2026-07-28 `_meta` must be answered.
  - `legacy_post_init`: these cases share one process per revision, which
    first runs `initialize` with that revision and
    `notifications/initialized`. After each case, `tools/list` must be
    answered.
  - `legacy_pre_init`: each case gets a fresh process per revision. After the
    case, `initialize` with that revision must succeed.
- `revision`: legacy phases only. A revision, or `"*"` for every legacy
  revision the server advertises (`reshiki_mcp::server::SUPPORTED` after
  2026-07-28: 2025-11-25 and 2025-06-18).
- `platform` (optional): `"unix"` runs the case only on Unix.
- `within_ms` (optional): the longest the replies may take.
- `attempts` (optional): how many times the case may be sent. While attempts
  are left and every reply is a result, the case is sent again after the
  sentinel instead of being checked. `duplicate_in_flight_id` needs it: the
  framing frees an id once its response is written, so a compose that ends
  before its duplicate is read makes the duplicate a legal reuse, answered
  normally. The shared process reads stdout all the time, so nothing can hold
  that response back; `a_duplicate_in_flight_id_is_an_invalid_request` in
  tests/agent_api_runtime.rs retries the same way.
- `source`: where the expected outcome comes from; see below.
- `lines`: what is written, in one write. Each piece ends in a newline, which
  is added when it has none.
  - A string is sent as UTF-8.
  - `{"base64": …}` is sent as the decoded bytes, for invalid UTF-8.
  - `{"json": …}` is sent as that value's compact JSON.
  - An array holds several of these pieces.
- `expect`: one expectation, an array of them, or `"silent"`.
  - `"silent"`: nothing is written for the case.
  - `{"error": code, "id": …, "message": …, "data": …}`: a JSON-RPC error.
    Without `id` the response must have no `id` member. `message` and `data`
    are compared when given. A -32022 error's `data.supported` must also equal
    the advertised revisions.
  - `{"result": true, "id": …}`: a result that is not a tool error. An object
    in place of `true` lists fields the result must have.
  - `{"tool_error": code, "prefix": …, "id": …, "message": …, "contains": …}`:
    a `tools/call` result with `isError: true` and
    `structuredContent.error.code` equal to the operation error code. `prefix`
    is the access error code that starts the message (`"{prefix}: …"`), or
    `null`. `message` is the whole message and `contains` a part of it, each
    checked when given.

  Replies sharing an id may arrive in any order. Whatever the expectation,
  nothing else may arrive before the sentinel is answered.

Placeholders in `lines` are quoted strings, `"${kind:argument}"`. Each one,
quotes included, is replaced by the JSON it stands for:

- `"${meta}"`: the 2026-07-28 `_meta` object.
- `"${revision}"`: the process's legacy revision.
- `"${in:NAME}"`, `"${outside:NAME}"`: the path of `NAME` in the granted read
  folder, or in a folder that is not granted.
- `"${fixture-text:NAME}"`, `"${fixture-base64:NAME}"`: a fixture in this
  folder, as text or as base64.
- `"${generated:NAME}"`: a payload made at test time.
  - `depth-200`: arrays nested 200 deep.
  - `compose-32`: the compose arguments for 32 molecules of
    tests/common/headless.rs, slow enough for a duplicate id to arrive while
    the call runs.
  - `cdx-over-budget`: a cdx text one byte over `Budgets::max_cdx_base64`.
  - `picture-30000`: a native drawing whose embedded PNG declares
    30000×30000 pixels.
  - `objects-over-budget`: a native drawing of `Budgets::max_objects` + 1
    atoms.

Each process may read `<tree>/in` and write `<tree>/out`. `in` holds the
folder `folder.mol` and, on Unix, the FIFO `pipe.mol`. `outside/eth.mol` exists
but is not granted. RESHIKI_DATA_DIR, HOME, USERPROFILE, APPDATA and
LOCALAPPDATA point into the same tree. When its input ends, each process must
exit 0 within 10 s. Every stdout line must be one JSON-RPC 2.0 object, stderr
must have no `panicked at`, and nothing outside `<tree>/out` may be created,
removed or changed. A watchdog kills a process that hangs. It restarts for
each attempt of a case with the time the send, each reply and the sentinel
may take, 30 s each, so a shared process lives as long as its cases need.

## Provenance

Each case's `source` names where its outcome comes from:

- `framing:*`: the error-code contract in crates/mcp/src/framing.rs and its
  classifier crates/mcp/src/framing/head.rs, tested in
  crates/mcp/src/framing/tests.rs and crates/mcp/tests/transcripts/errors.jsonl.
  - `parse`: not UTF-8 or not JSON, including nesting deeper than serde_json's
    recursion limit, is -32700 `Parse error` without an id.
  - `not-object`: JSON that is not an object is -32600 without an id.
  - `invalid-id`: an id that is a float, a number outside i64, a bool, null,
    an object or a string over 256 bytes is -32600 without an id.
  - `valid-id`: the largest i64 is a valid id.
  - `version-or-method`: `jsonrpc` other than "2.0", or `method` missing or not
    a string, is -32600 echoing the id.
  - `duplicate`: a duplicate in-flight id is -32600 `Duplicate request id`.
  - `ignored`: client responses and unknown notifications get no output.
  - `cancel`: a malformed cancellation, or one for an unknown or completed
    request, gets no output.
  - `crlf`, `blank`: one trailing `\r` is stripped, and empty lines are
    skipped.
- `protocol:*`: crates/mcp/src/server.rs and the transcripts in
  crates/mcp/tests/transcripts.
  - `method-not-found`: JSON-RPC 2.0's -32601.
  - `invalid-params`: -32602 for an unknown tool, malformed `tools/call` or
    `initialize` params, or a missing or malformed `_meta`
    (https://modelcontextprotocol.io/specification/2026-07-28/basic/index).
  - `unsupported-version`: -32022 with `data.supported`
    (https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning).
  - `result`: an ordinary answered request.
- `lifecycle:*`: the legacy lifecycle.
  - `fallback`: `initialize` with a revision that has no handshake of its own
    is answered with 2025-11-25, as in `initialize_falls_back_to_2025_11_25` of
    crates/mcp/tests/transcripts.rs.
  - `observed`: no transcript covers it, so the corpus records the binary's
    behavior. A legacy `tools/call` before `initialize` has no `_meta` and gets
    -32602, and `initialize` still works afterwards. A second `initialize` is
    answered like the first.
- `ops:CODE`: `reshiki_agent::ops::error::ErrorKind::code()` for the tool
  errors of the operation decoders and operations: import, inspect and compose
  in crates/agent/src/ops, and the object ID and handle rules in
  crates/agent/src/ops/wire.rs.
- `access:CODE`: an `AccessError` code (crates/agent/src/access.rs) as the
  message prefix under its operation code, as `From<AccessError> for OpError`
  maps it (crates/agent/src/ops/error.rs). `not_a_regular_file` is `failed`,
  and `path_not_granted` is `access_denied`. crates/agent/src/access/read.rs
  maps Windows' denial to open a directory to `not_a_regular_file`, so the
  directory case expects it on every platform.
- `proposal:validate`: `Proposal::validate` in crates/agent/src/lib.rs, whose
  message compose returns as `invalid_arguments`.

## Where the implementation differs from the step plan

The step plan (safety-6) predates transport-2. The owner decided that the
framing table as implemented wins, so the corpus records it:

- Invalid JSON, a truncated object, invalid UTF-8 and a raw NUL get -32700
  without an id, not silence.
- A top-level array gets -32600 without an id, not silence.
- A UTF-8 BOM before a valid request gets -32700 without an id, because
  serde_json does not skip a BOM.
- An id that is an object, a float or a bool gets -32600 without an id, not
  silence.
- A whitespace-only line gets -32700. Only empty lines are skipped.

## Committed fixtures

Each committed fixture stays under 4 KB. Larger inputs are generated at test
time.

- `truncated.cdx`: the first 300 bytes of
  tests/fixtures/bold-arene-clipboard.cdx (1089 bytes). The CDX header is
  intact, but the object tree is cut off, so the import fails with `failed`.
- `deep.cdxml`: an XML declaration and a `CDXML` root whose `page` holds 240
  nested `group` elements. That is deeper than the CDXML importer's limit of 64
  (crates/io/src/chemistry/cdxml/parse.rs), so the import fails with `failed`
  instead of recursing.

## CLI corpus

`cli_corpus` in the same file needs no manifest. It follows the CLI grammar in
src/cli/args.rs and the exit codes in src/cli.rs: 2 for a usage error and 1 for
a failure. Each run must exit within 30 s, so a GUI started by mistake fails
by timeout. It must also print nothing on stdout and leave the working folder
unchanged.

- An unknown command, `convert` without an input, and `--to xyz` exit 2.
- A missing input, a folder as input, an input over `Budgets::max_text_bytes`,
  and an existing output without `--force` exit 1.
