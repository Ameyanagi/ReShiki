# Agent API P1 acceptance record

This record defines acceptance gates G1-G8 for P1 of the experimental
[agent API](agent-api.md), `reshiki --mcp` and `reshiki --cli`, and collects
their evidence. Each gate names its commands, what counts as a pass, and slots
for run URLs, commit SHAs, client versions and dates. A gate passes only when
every row in it has passed. The experimental label can be removed only after a
stable release ships with all eight gates passing
([experimental status](agent-api.md#experimental-status)).

P1 landed on `main` at `cfeec8fb` on 2026-10-08, as the merge of
[PR #269](https://github.com/Ameyanagi/ReShiki/pull/269). Its tip `7cf6cada`
has the same tree. The two Windows home-grant fixes landed with it: fix-1
(`916a5593`) and fix-2 (`93bd12a8`). The stack was based on `40c8d82a`.

**Current status (2026-10-13):** the CI and packaged-build rows are filled from
the [CI and Nightly runs](#ci-and-nightly-runs) below. The client checks of
G1(d), G7's privacy sign-off, G8's real-app check and binary-size review, and a
gate-script run on the landed tip are still **Pending**; each one carries an
owner TODO.

## CI and Nightly runs

| Run                                                                          | Workflow                                                  | Source                                           | Date (UTC) | Result                                                                                                                                                                                                                                               |
| ---------------------------------------------------------------------------- | --------------------------------------------------------- | ------------------------------------------------ | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [37766904478](https://github.com/Ameyanagi/ReShiki/actions/runs/37766904478) | checks.yml on the push to `main`                          | `cfeec8fb`                                       | 2026-10-08 | Every job passed, including the rust job on macos-14, windows-2022 and ubuntu-22.04                                                                                                                                                                  |
| [37758833095](https://github.com/Ameyanagi/ReShiki/actions/runs/37758833095) | checks.yml, run by hand with the live RDKit reference     | `7cf6cada`, the P1 tip (same tree as `cfeec8fb`) | 2026-10-08 | All 12 reference shards passed                                                                                                                                                                                                                       |
| [37853374852](https://github.com/Ameyanagi/ReShiki/actions/runs/37853374852) | nightly.yml, which calls release.yml with `nightly: true` | `51fa0991`, which contains `cfeec8fb`            | 2026-10-08 | All six package builds, the four sign jobs and publish passed. It published [0.11.0-nightly.20261008.37853374852.1](https://github.com/Ameyanagi/ReShiki/releases/tag/nightly-0.11.0-nightly.20261008.37853374852.1), the first Nightly with the API |

Before landing, checks run
[37751648802](https://github.com/Ameyanagi/ReShiki/actions/runs/37751648802)
and release run
[37751653086](https://github.com/Ameyanagi/ReShiki/actions/runs/37751653086) on
`45db305d` failed: every reference shard refused the new workspace test targets,
and both Windows builds failed in the safety-7 step. `7cf6cada` fixed both, and
release run
[37758838460](https://github.com/Ameyanagi/ReShiki/actions/runs/37758838460)
then passed on the P1 tip.

The [published Windows Nightly review](changes/nightly-windows-20261011.md) adds
a manual Windows x64 check of Nightly 0.11.0-nightly.20261011.38123945065.1:
CLI information, ethanol conversion, MOL analysis, `.rsk` rendering and
graphics information all exited zero.

## Status

| Gate                                             | Covers                                 | Local                  | CI                   | Manual               |
| ------------------------------------------------ | -------------------------------------- | ---------------------- | -------------------- | -------------------- |
| [G1](#g1-protocol-interop)                       | Interop in three MCP revisions         | Passed (a), (c)        | Passed (a), (b), (c) | Pending (owner): (d) |
| [G2](#g2-packaged-headless-startup)              | Packaged headless startup              | Not run locally        | Passed               | Pending (owner)      |
| [G3](#g3-filesystem-escape-suite)                | Filesystem escape suite                | Passed (Unix cases)    | Passed               | —                    |
| [G4](#g4-quota-and-cancellation)                 | Quota and cancellation                 | Passed, except Windows | Passed               | —                    |
| [G5](#g5-malformed-input)                        | Malformed input                        | Passed                 | Passed               | —                    |
| [G6](#g6-dependencies-and-licenses)              | Dependencies and licenses              | Passed                 | Passed               | —                    |
| [G7](#g7-docs-and-labels)                        | Docs and labels                        | Passed                 | —                    | Pending (owner)      |
| [G8](#g8-no-regression-in-existing-app-behavior) | No regression in existing app behavior | Passed (local checks)  | Passed               | Pending (owner)      |

## Local checks

`scripts/agent_api_gates.py` runs every gate check a development machine can
run, in order: the G3-G7 cargo and uv commands, then the G8 audit, diff and
grep checks. It prints a pass/fail table and each command line, and exits 1 if
a check fails. A test command that runs no test fails, so a mistyped filter
cannot pass. CI and manual evidence is listed as pending; the script never
claims it.

```sh
export RESHIKI_INCHI_HELPER="$PWD/artifacts/inchi-helper/reshiki-inchi-helper"
export RESHIKI_REQUIRE_INCHI_HELPER=1
uv run --no-project python scripts/agent_api_gates.py --local
```

Without `--local` the script only lists the commands and the pending evidence.
`--base` sets the commit the G8 history and diff checks compare with. The
default is `40c8d82a`, `main` before P1, so the checks still see the stack's
changes after it lands on `main`.
Windows-only code (the Windows escape cases, the pipe tests of G4) does not
build on macOS or Linux, so its evidence comes from CI.

| Run                       | Date (UTC) | Machine                                  | Source                                      | Result                                                       |
| ------------------------- | ---------- | ---------------------------------------- | ------------------------------------------- | ------------------------------------------------------------ |
| Stack tip, before landing | 2026-10-08 | macOS 26.5.1, Apple Silicon, Rust 1.99.0 | `9023a210`, plus this record and its script | All 17 checks passed                                         |
| Landed tip                | Pending    | Pending                                  | Pending                                     | Pending. TODO (owner): run the script on `cfeec8fb` or later |

## G1: protocol interop

The server must work in every MCP revision it advertises: 2026-07-28,
2025-11-25 and 2025-06-18
([versioning](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning)).
It accepts no older revision; an `initialize` that asks for 2025-03-26 gets
2025-11-25. [Protocol revisions](agent-api.md#protocol-revisions) lists the
tests of each revision.

| Check                      | Command                                                                                                                                           | Passes when                                                                                                                                                                                                                                                                                                                         | Local                                  | CI                                                                                                                          |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| (a) Golden transcripts     | `cargo test --locked -p reshiki-mcp --test transcripts`; `cargo test --locked --no-default-features -p reshiki --test mcp_stdio --test mcp_tools` | In every revision, each reply equals its golden line as parsed JSON, not byte for byte; `mcp_stdio` and `mcp_tools` first replace the app version, tool definitions, image and file data and document handles with placeholders. Nothing else is written                                                                            | Passed: 18, 14 and 4 tests, 2026-10-08 | Passed: [run 37766904478](https://github.com/Ameyanagi/ReShiki/actions/runs/37766904478), rust job on macOS, Windows, Linux |
| (a) rmcp client interop    | `the_rmcp_client_interoperates` in `tests/mcp_stdio.rs`, with the [rmcp](https://github.com/modelcontextprotocol/rust-sdk) 3.5.1 client           | The client discovers 2026-07-28, lists the catalog's tools, calls `info`, and the server exits 0 when the client closes stdin                                                                                                                                                                                                       | Passed, in `mcp_stdio` above           | Passed: same run                                                                                                            |
| (b) Packaged stdlib driver | `verify_agent_api` (`scripts/check_runtime_dependencies.py`) in release.yml's "Build and test extracted portable archive" step                    | `scripts/agent_api_client.py` runs every revision against each of the six packages, and the step prints `Packaged MCP server and CLI passed`                                                                                                                                                                                        | Not run locally                        | Passed: [Nightly run 37853374852](https://github.com/Ameyanagi/ReShiki/actions/runs/37853374852), all six package builds    |
| (c) Phase-aware corpus     | `cargo test --locked --no-default-features -p reshiki --test agent_api_malformed`                                                                 | Each case runs only in its declared phase: the modern cases in 2026-07-28, and each legacy case before or after `initialize`, as it declares, in the older revisions it names (every current one names 2025-11-25 and 2025-06-18); one modern case runs on Unix only. Each gets its expected reply and the next request is answered | Passed, see [G5](#g5-malformed-input)  | Passed: [run 37766904478](https://github.com/Ameyanagi/ReShiki/actions/runs/37766904478), rust job on macOS, Windows, Linux |

(d) Each client is configured with the snippet from
[Connect AI agents](https://reshiki.com/guide/agents/#3-connect-your-client),
with one folder granted for writing. The server does not log the negotiated
revision, even at `--log-level debug`, so the revision comes from the
client's own log or a captured `initialize` or `server/discover` exchange. The
owner asks the client to list ReShiki's tools, render a SMILES as a PNG,
cancel a render from the client's UI, and save a file into the granted folder
and then outside it.

TODO (owner): fill in this table from your own client checks.

| Observation                                         | Claude Code | Claude Desktop, macOS | Claude Desktop, Windows | Codex CLI |
| --------------------------------------------------- | ----------- | --------------------- | ----------------------- | --------- |
| Client version                                      | Pending     | Pending               | Pending                 | Pending   |
| System and ReShiki build (SHA)                      | Pending     | Pending               | Pending                 | Pending   |
| Snippet used (guide section)                        | Claude Code | Claude Desktop        | Claude Desktop          | Codex     |
| Revision observed, and where                        | Pending     | Pending               | Pending                 | Pending   |
| All 13 tools listed                                 | Pending     | Pending               | Pending                 | Pending   |
| Rendered image visible in the client                | Pending     | Pending               | Pending                 | Pending   |
| Render cancelled from the client UI, no reply       | Pending     | Pending               | Pending                 | Pending   |
| File written in the granted folder only             | Pending     | Pending               | Pending                 | Pending   |
| Save outside the folder refused, `path_not_granted` | Pending     | Pending               | Pending                 | Pending   |
| Date checked                                        | Pending     | Pending               | Pending                 | Pending   |

## G2: packaged headless startup

| Check                                 | Command                                                                                       | Passes when                                                                                                                                                                                                 | Evidence                                                                                                                                                                                                                                |
| ------------------------------------- | --------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Six packaged builds                   | `gh workflow run release.yml --ref main -f nightly=true` ([landing task 4](#landing-tasks))   | All six build jobs succeed, including `verify_agent_api` in each "Build and test extracted portable archive" step and on the installed Windows and macOS DMG copies                                         | Passed: [Nightly run 37853374852](https://github.com/Ameyanagi/ReShiki/actions/runs/37853374852), `51fa0991`, 2026-10-08                                                                                                                |
| Quarantined signed app                | The sign jobs of a release.yml run on `main` (`sign_macos=true`, or `nightly=true` on `main`) | `verify_archive(signed=True)` launches a quarantined copy that serves MCP and prints `Quarantined app copy served MCP.` for macos-arm64 and macos-x64                                                       | Passed: both macOS sign jobs of [Nightly run 37853374852](https://github.com/Ameyanagi/ReShiki/actions/runs/37853374852) printed `Quarantined app copy served MCP.`                                                                     |
| Windows upgrade over a running server | The Windows installer check in the x64 and ARM64 build jobs of the nightly run                | `verify_windows_upgrade_with_running_agent` prints `Windows upgrade with a running MCP server: <outcome>`; either outcome keeps a working install and uninstaller. Record the outcome for each architecture | [Nightly run 37853374852](https://github.com/Ameyanagi/ReShiki/actions/runs/37853374852): `setup failed (5) and kept the old install.` on x64 and on ARM64. Both jobs then verified installation, upgrade, chemistry and uninstallation |

## G3: filesystem escape suite

| Check                              | Command                                                                                  | Passes when                                                                                                                                                                                                                                                                                                                 | Local                     | CI                                                                                                                                                               |
| ---------------------------------- | ---------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Escape suite                       | `RESHIKI_REQUIRE_LINK_TESTS=1 cargo test --locked -p reshiki-agent access::escape_tests` | Every case the command runs passes: the Unix cases on macos-14 and ubuntu-22.04, the Windows cases on windows-2022. It runs the 2,000-read directory-swap race, not the ignored 200,000-read one. With `RESHIKI_REQUIRE_LINK_TESTS=1`, a Windows account that cannot create symlinks fails the suite instead of skipping it | Passed on macOS: 15 tests | Passed: [run 37766904478](https://github.com/Ameyanagi/ReShiki/actions/runs/37766904478), rust job on macOS, Windows, Linux, with `RESHIKI_REQUIRE_LINK_TESTS=1` |
| Home-folder grant refusal, Windows | `cargo test --locked --no-default-features -p reshiki --test agent_api_grants`           | `granting_the_home_folder_exits_2` and `granting_the_real_profile_folder_exits_2` pass on windows-2022. fix-1 grants the child's own home folder; fix-2 refuses every home candidate (the profile known folder and `USERPROFILE`), where the old lookup also needed the AppData folders and failed open                     | Passes on macOS: 10 tests | Passed: [run 37766904478](https://github.com/Ameyanagi/ReShiki/actions/runs/37766904478), windows-2022; before landing, PR #268 CI (run 37747164384)             |

The ignored 200,000-read race is extra stress evidence outside the gate, and
CI does not run it. It passed on macOS on 2026-10-08 with
`cargo test --locked -p reshiki-agent access::escape_tests::unix::a_directory_swapped_for_a_symlink_mid_read_never_yields_outside_bytes_long -- --exact --ignored`.

## G4: quota and cancellation

G4 cites the tests that own each behavior instead of repeating them.

| Owner       | Command                                                                                                                              | Passes when                                                                                                                                                                                                                                 | Local            | CI                                                                                                                                            |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| ops-6       | `cargo test --locked -p reshiki-agent ops::exec`                                                                                     | A cancelled or dropped call keeps its permit until its blocking job ends, a full queue answers `busy`, a passed deadline times out at the next checkpoint, a cancel after the effect keeps the effect, and panics become internal errors    | Passed: 18 tests | Passed: [run 37766904478](https://github.com/Ameyanagi/ReShiki/actions/runs/37766904478), rust job on macOS, Windows, Linux                   |
| ops-12(c)   | `cargo test --locked -p reshiki-agent ops::headless::tests::cancelled_queued_calls_end_cancelled_while_the_rest_complete -- --exact` | Of six renders queued behind two blocked permits, the three cancelled return `Cancelled`, the other three succeed, `drained()` resolves and both permits return                                                                             | Passed: 1 test   | Passed: same run                                                                                                                              |
| transport-5 | `cargo test --locked -p reshiki-mcp`                                                                                                 | With the test `FakeHost`: a busy tool is an `isError` result in every revision, a panicking tool gets -32603 and serving goes on, a cancelled call sends nothing, a late cancel is ignored, and 200 cancel cycles leave nothing outstanding | Passed: 76 tests | Passed: same run                                                                                                                              |
| safety-5    | `cargo test --locked --no-default-features -p reshiki --test agent_api_runtime`                                                      | Through the binary: `busy` only beyond the executor's queue, at most one reply per cancelled render, a duplicate in-flight id is -32600, oversize lines are refused, and `RESHIKI_AGENT_HEAP_MB` outside 256-16384 exits 2                  | Passed: 6 tests  | Passed: same run                                                                                                                              |
| safety-7    | `cargo test --release --locked --no-default-features -p reshiki --test agent_api_windows_pipes --target <target>` in release.yml     | Stdout reaches end of file after a call that started a worker and while a worker outlives the server, on x64 and ARM64 in the release profile                                                                                               | Windows only     | Passed: [Nightly run 37853374852](https://github.com/Ameyanagi/ReShiki/actions/runs/37853374852), x64 and ARM64 (2 passed, 1 ignored on each) |

## G5: malformed input

| Check              | Command                                                                           | Passes when                                                                                                                                                                                                                                                                                                                                                                                                 | Local           | CI                                                                                                                          |
| ------------------ | --------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------- | --------------------------------------------------------------------------------------------------------------------------- |
| MCP and CLI corpus | `cargo test --locked --no-default-features -p reshiki --test agent_api_malformed` | Each of the 40 or more MCP cases in `tests/fixtures/agent-api/malformed/cases.json` gets its expected reply and the next request is answered; at end of input the server exits 0 within 10 s, stdout holds only JSON-RPC objects, stderr has no `panicked at` and nothing changes outside the output folder. Each malformed CLI command exits 2 or 1 within 30 s with nothing on stdout and no file changed | Passed: 5 tests | Passed: [run 37766904478](https://github.com/Ameyanagi/ReShiki/actions/runs/37766904478), rust job on macOS, Windows, Linux |

## G6: dependencies and licenses

| Check                 | Command                                                                                                                            | Passes when                                                                                                                                                                                                                                             | Local                   | CI                                                                                                                       |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| Dependency audit      | `uv run --no-project python scripts/check_agent_dependencies.py`                                                                   | It prints `Agent dependencies passed for 6 release targets.`: every package added since `40c8d82a` has an allowed license, rmcp keeps its reviewed server-only features and no HTTP, TLS or WebSocket crate, and serde_json keeps its baseline features | Passed                  | Passed: [run 37766904478](https://github.com/Ameyanagi/ReShiki/actions/runs/37766904478), macOS rust job                 |
| Audit in `notices()`  | `uv run --locked python -m unittest tests.test_agent_dependencies tests.test_release tests.test_license_notices`                   | The tests pass, including `notices()` in `scripts/build_release.py` running the audit before it writes the notices; in release.yml, every package build passes it                                                                                       | Passed: 27 tests        | Passed: [Nightly run 37853374852](https://github.com/Ameyanagi/ReShiki/actions/runs/37853374852), all six package builds |
| License consolidation | `consolidated_notices()` from `scripts/license_notices.py` over `cargo metadata --locked`; `agent_api_gates.py` prints the command | It exits 0, so every Cargo package's notice texts are found and consolidated                                                                                                                                                                            | Passed: 3,787,219 bytes | Passed: [Nightly run 37853374852](https://github.com/Ameyanagi/ReShiki/actions/runs/37853374852), all six package builds |

## G7: docs and labels

| Check                   | Command                                                                                                       | Passes when                                                                                                                                                      | Local                          | Owner                                |
| ----------------------- | ------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ | ------------------------------------ |
| Experimental labels     | `cargo test --locked --no-default-features -p reshiki --test agent_api_labels`                                | `server/discover`, `initialize`, `tools/list`, `info`, `--cli help` and CLI JSON carry the labels of `crates/agent/src/stability.rs` and every tool's four hints | Passed: 4 tests                | —                                    |
| Filesystem fence        | `uv run --locked python -m unittest tests.test_agent_fs_fence`                                                | Each agent crate's `clippy.toml` disallows direct filesystem calls, and the agent, MCP and launch sources use no fenced token outside the access module          | Passed: 5 tests                | —                                    |
| Documentation build     | `bun run docs:build`                                                                                          | Every page builds and the link check prints `Verified <n> local documentation links and assets.`                                                                 | Passed: 8,276 links and assets | —                                    |
| Privacy-policy sign-off | The owner reviews [the agent API section](privacy-policy.md#optional-agent-api-and-command-line-experimental) | The owner confirms it matches the behavior above: grants for MCP, the user's authority for the CLI, no listener, no network, and what clients may forward        | —                              | Pending. TODO (owner): name and date |

## G8: no regression in existing app behavior

| Check                      | Command                                                                                                                | Passes when                                                                                                                                                                                                                                                                    | Evidence                                                                                                                                                |
| -------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Full checks                | checks.yml on the push of the tip to `main` ([landing task 2](#landing-tasks))                                         | Every job passes on macos-14, windows-2022 and ubuntu-22.04                                                                                                                                                                                                                    | Passed: [run 37766904478](https://github.com/Ameyanagi/ReShiki/actions/runs/37766904478), `cfeec8fb`, 2026-10-08                                        |
| Live reference             | `gh workflow run checks.yml --ref main -f live_reference=true` (landing task 3)                                        | All 12 reference shards (4 per OS) pass                                                                                                                                                                                                                                        | Passed: [run 37758833095](https://github.com/Ameyanagi/ReShiki/actions/runs/37758833095) on the P1 tip `7cf6cada` (same tree as `cfeec8fb`), 2026-10-08 |
| Existing package checks    | release.yml `nightly=true` (landing task 4)                                                                            | `verify_archive` still runs the engine check twice, the InChI and geometry workers and the macOS workers (`scripts/build_release.py:549-558`), and they pass on all six targets. Locally, `git diff 40c8d82a...HEAD -- scripts/build_release.py` adds 4 lines and removes none | Passed: [Nightly run 37853374852](https://github.com/Ameyanagi/ReShiki/actions/runs/37853374852), six targets                                           |
| Codex byte pins            | `cargo test --locked --no-default-features -p reshiki --test assistant_contract`                                       | ops-1's pins of the Codex tool JSON and the Proposal and critique schemas hold                                                                                                                                                                                                 | Local: passed, 5 tests                                                                                                                                  |
| Pins unchanged after ops-1 | `git log 40c8d82a..HEAD` on the three pinned fixtures and on `tests/assistant_contract.rs`, as the gate script runs it | The fixtures' only commit is ops-1, and the test's only later change is ops-3 swapping `definitions()` for `codex::dynamic_tools(&SPECS)`                                                                                                                                      | Local: passed                                                                                                                                           |
| serde_json feature audit   | The G6 dependency audit                                                                                                | serde_json features on every target stay within `scripts/agent_dependency_baseline.json` plus the reviewed `alloc`, without `preserve_order` or `arbitrary_precision`                                                                                                          | Local: passed, see [G6](#g6-dependencies-and-licenses)                                                                                                  |
| GUI sources                | `git diff 40c8d82a...HEAD -- src/app.rs src/app src/canvas.rs src/canvas`, as the gate script runs it                  | `src/canvas` is unchanged; `src/app` changes only by the reviewed edits below                                                                                                                                                                                                  | Local: passed                                                                                                                                           |
| Startup dispatch order     | `git grep` of `src/main.rs`, as the gate script runs it                                                                | Worker flags come first, then `launch::mode` for `--mcp` and `--cli`, then the Windows `--graphics-info` and Office registration, `--engine-check` and the GUI, each exactly once                                                                                              | Local: passed                                                                                                                                           |
| Real-app GUI check         | The codex-computer-use skill on a debug build of `main` (landing task 5)                                               | The app opens, draws, saves and reopens an `.rsk`, exports, and opens the Codex panel as before                                                                                                                                                                                | Pending. TODO (owner): date, SHA, notes                                                                                                                 |
| Binary size                | The executables in the nightly artifacts of the tip and of the last nightly before landing                             | The size change is recorded for each target below and the owner accepts it                                                                                                                                                                                                     | Pending. TODO (owner): accept the size change                                                                                                           |

The reviewed GUI changes are ops-4's characterized extractions in
`src/app/figure_export.rs` (13 lines added, 16 removed), `src/app/files.rs` (1
and 3) and `src/app/inspector.rs` (4 and 19, including ops-9's `pub(super)` on
two helpers); the `#[cfg(test)] mod ops_parity_tests;` registration in
`src/app.rs`; and the test modules `src/app/ops_parity_tests.rs`,
`src/app/files/tests.rs` and `src/app/inspector/tests.rs`. The gate script
pins the blob id of each reviewed source, so any other change to those four
files fails it, even one that keeps the line counts; only the test modules may
change.

Compare the executables of 0.11.0-nightly.20261007.37701212391.1 (`40c8d82a`,
the last Nightly before landing) with those of
0.11.0-nightly.20261008.37853374852.1 (`51fa0991`, the first Nightly with the
API). TODO (owner): record the sizes below.

| Target                      | Last nightly before landing | Tip     | Change  |
| --------------------------- | --------------------------- | ------- | ------- |
| `aarch64-apple-darwin`      | Pending                     | Pending | Pending |
| `x86_64-apple-darwin`       | Pending                     | Pending | Pending |
| `x86_64-pc-windows-msvc`    | Pending                     | Pending | Pending |
| `aarch64-pc-windows-msvc`   | Pending                     | Pending | Pending |
| `x86_64-unknown-linux-gnu`  | Pending                     | Pending | Pending |
| `aarch64-unknown-linux-gnu` | Pending                     | Pending | Pending |

## Landing tasks

P1 landed as the merge of PR #269, not as a fast-forward. Status of each task
on 2026-10-13:

1. Done: fix-1 (`p1/fix-windows-home-grant`, `916a5593`) and fix-2
   (`p1/fix-home-folder-sources`, `93bd12a8`) landed with P1. They fix the
   Windows home-grant refusal that G3 lists.
2. Done: checks.yml ran on the push of `cfeec8fb`
   ([run 37766904478](https://github.com/Ameyanagi/ReShiki/actions/runs/37766904478)): G8's full checks and the CI rows of G1, G3, G4
   and G5.
3. Done on the P1 tip, with the same tree as `cfeec8fb`
   ([run 37758833095](https://github.com/Ameyanagi/ReShiki/actions/runs/37758833095)): G8's live reference.
4. Done through the first Nightly with the API
   ([run 37853374852](https://github.com/Ameyanagi/ReShiki/actions/runs/37853374852)), which also signed the macOS packages: G1(b),
   G2, G4's Windows pipe tests, G6 and G8's package checks. Binary sizes are
   still open (task 6).
5. Pending. TODO (owner): check the real app on a debug build of `main` with
   the codex-computer-use skill: G8.
6. Pending. TODO (owner): complete G1(d), G7's privacy sign-off and G8's
   binary size review.
7. Done for the runs above, including the failed pre-landing runs and their
   fix. TODO (owner): record a gate-script run on the landed tip.
8. Done: `docs/agent-api.md`, `docs/architecture.md`, `docs/feature-status.md`,
   `docs/privacy-policy.md`, `docs/runtime-safety.md`, `docs/windows.md`,
   `docs/changes-unreleased.md` and `website/src/content/docs/guide/agents.mdx`
   say "Nightly builds since 2026-10-08", the date of the first Nightly with
   the API.
9. Done for the [status](#status) table. The
   [client compatibility](agent-api.md#client-compatibility) table stays
   Pending until the owner completes G1(d).
