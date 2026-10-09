# Local chemical naming: current-source validation

Supported names become editable structures through pinned local OPSIN 2.9.0;
supported organic graphs receive names from original Rust rules, accepted only
after exact reconstruction by the local parser. Both directions stay on the
computer. Java 11+ HotSpot must already be installed. Reverse coverage is a
declared subset, not every IUPAC or preferred IUPAC name (PIN). See the
[coverage guide](../chemical-naming.md).

This supplemental review validates production source
`d9df649129c5144cf304df7295dc573825dcbe49`, including the executor fix
`85adb8c2` and process fix `d9df6491`. The broader stereo, caption and rejection
[desktop review](../chemical-naming-local-visual-review.md) remains tied to its
original `aaa6f9b7` source and signed `1c7d…` app. Those captures, native files,
compiler proofs and immutable links are unchanged. The older HTTP prototype
remains [superseded historical evidence](../chemical-naming-visual-review.md).

## Corrected process and admission behavior

The earlier [3706 CI run](https://github.com/Ameyanagi/ReShiki/actions/runs/37954198084)
actually failed distinct tests after successfully selecting Java and building
the app helper. macOS failed agent API admission at the expected initial
capacity; Linux failed two real-parser tests on resident-memory sampling;
Windows failed three real-parser/native-worker structured-output tests.
These were application runtime failures, separate from the earlier workflow
context and Java-selector setup failures. The original logs remain preserved;
the [compact CI record](../../tests/fixtures/chemical-naming/process-d9df6491/ci-native-summary.json)
includes their URLs and hashes.

- **Admission:** a worker could acquire an execution permit before decrementing
  the waiting count. The old waiting-only admission gate could then reject a
  request while total live work was below concurrency plus queue capacity.
  The correction atomically bounds total admitted calls. A controlled acquired
  hook reproduces the old failure and passes the correction; all 20 executor
  tests and all six unchanged runtime integration tests pass, including the
  original initial-ten-call capacity assertion. The
  [local receipt](../../tests/fixtures/chemical-naming/process-d9df6491/admission-validation-summary.json)
  records both results. No assertion was weakened.
- **Linux RSS:** a kernel process snapshot can omit `VmRSS` after its memory
  context is released during exit, before the owned child becomes waitable.
  Absence now follows the existing bounded transient recheck. Present malformed
  or overflowing values and positively measured memory excess remain immediate
  failures. Pure missing/invalid snapshots and a real owned, unreaped exited
  child exercise this distinction; no zero-RSS substitution or limit relaxation
  was added. The kernel's [status generation](https://github.com/torvalds/linux/blob/v6.11/fs/proc/array.c)
  and [exit order](https://github.com/torvalds/linux/blob/v6.11/kernel/exit.c)
  explain the legal transition.
- **Windows pipes:** Rust's file-read normalization could turn a connected
  nonblocking pipe's no-data status into an apparent EOF before the worker
  produced its response. Direct synchronous `ReadFile` preserves no-data as
  `WouldBlock` and reserves EOF for a broken pipe. An empty caller buffer returns
  immediately; a peer zero-byte write leaves a connected nonempty read pending.
  Actual gated child stdout/stderr and two-part pipe tests cover idle intervals,
  both chunks and real EOF. Job creation, resource limits, cancellation, output
  bounds and owned cleanup remain in place. The
  [independent source review](../../tests/fixtures/chemical-naming/process-d9df6491/windows-readfile-independent-review.json)
  links the Rust and Microsoft primary contracts.

The old CI logs did not capture the Linux PID state or Windows response bytes.
The verified source counterexamples and controlled regressions establish legal
failure paths; they do not retroactively observe the precise old interleaving.
These changes affect admission and transport, so a drawing comparison cannot
demonstrate the correction. The original real naming, identity, stereo and
capacity tests remain intact.

## Exact-source tests, CI and payload

The [completed d9 CI run](https://github.com/Ameyanagi/ReShiki/actions/runs/37959157305)
and associated checks finished with **22 success, three intentional skips and
zero failures**. Full logs independently establish pinned Temurin
`21.0.11+10-LTS`, the exact built app helper and real naming execution on
[macOS ARM64](https://github.com/Ameyanagi/ReShiki/actions/runs/37959157305/job/113917377806),
[Linux x64](https://github.com/Ameyanagi/ReShiki/actions/runs/37959157305/job/113917377592)
and [Windows x64](https://github.com/Ameyanagi/ReShiki/actions/runs/37959157305/job/113917377578).
All 14 forward graph/stereo references and 66 native-rule → local OPSIN → exact
graph references pass on each. macOS/Linux pass 31 naming tests each; Windows
passes 28, including its separately executed accessibility/scrolled Insert
renderer checks. All six matching-host ARM64/x64 process jobs pass the native
platform regressions. GNOME and Sway each pass 11 actual compositor checks.
This is the frozen production-source CI result; a later evidence-only commit
has its own CI status and is not relabeled as this tested head.
The native jobs checked out GitHub's PR synthetic merge
`86395fa5c6532e3cc98325e4b14e81d7cc3cab89`, associated with d9 and the declared
base; the locally compiled and signed app uses the exact d9 branch source.

Local macOS checks on d9 used installed Zulu `21.0.8+9-LTS`, locked dependencies
and explicit network denial: nine process, 22 naming and nine app-state/history
tests pass. The two opt-in renderer tests were not rerun locally; the Windows
CI checks above executed them. The suite includes actual local graph
reconstruction, semantic-loss canaries, cancellation/drop, supervisor death,
600 inherited descriptors, resource enforcement and bounded pipe completion.
Strict process Clippy passes all six compile targets and formatting passes.
Foreign-target compilation is separate from matching-host runtime execution.

All 1,082 owned Rust/Cargo inputs were refreshed without changing their bytes.
The final default-feature Mac app build records all **17 own artifacts across
14 packages as `fresh:false`**, with **574 hashed absolute depfile inputs**
matching this worktree's Git source. This is the complete selected Mac app
dependency graph; Linux/Windows adapter crates are not Mac app dependencies.
The [compiler proof](../../tests/fixtures/chemical-naming/process-d9df6491/own-compiler-fresh-proof.json)
and [bundle metadata](../../tests/fixtures/chemical-naming/process-d9df6491/bundle-final-metadata.json)
retain that exact scope. A first verification-only assertion incorrectly
expected the older workspace build's artifact count; its
[receipt](../../tests/fixtures/chemical-naming/process-d9df6491/compiler-proof-first-count-assumption.json)
is retained separately from successful compilation and input verification.

The unique **ReShiki Local Naming Process Verified.app** passes deep/strict
signature verification. Its signed executable SHA256 is
`062b03f1d966e27301272a44cdbff243f002c2f59da1531dcd5d2f01f2b9dda5`;
raw executable SHA256 is
`a7626b7d7a5ea1a9e32bcf8f671362c57b19debe3eb5bb7ee4293e1bb1836925`.
All five packaged notice aggregates equal the earlier app's bytes, and all
39 original OPSIN/dependency records are present verbatim in the bundled
[notice proof](../../tests/fixtures/chemical-naming/process-d9df6491/opsin-license-byte-proof.json).
The old app is untouched. No Java/Python runtime, API fallback or global runtime
settings were added. Signed native-worker and chemistry checks also pass under
network denial; those headless checks do not invoke Java or replay the GUI.

## Supplemental native desktop smoke

The reviewer launched the new signed app through native desktop automation on
2026-10-10 Japan time (2026-10-09 UTC). Entering `ethanol` through
**Import → Chemical names… → Parse name locally** produced `CCO`, three atoms
and two bonds. **Insert editable structure**, complete selection and
**Generate name locally** produced `ethan-1-ol`, marked as verified by local
OPSIN 2.9.0. One header Undo emptied the main drawing; one Redo restored the
complete original native file. After quitting, a fresh process reopened it
with `C2H6O`, `CCO`, 46.069 g/mol and clean Undo/Redo history. Insert, Redo and
fresh Save As are byte-identical.

| Current signed app: local preview at 100%                                                                                                                        | Current signed app: local generation at 250%                                                                                                                              |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ![Current signed app parses ethanol locally into an editable CCO preview before insertion](../images/chemical-naming/process-d9df6491/ethanol-local-preview.jpg) | ![Current signed app generates ethan-1-ol for the complete three-atom two-bond ethanol selection](../images/chemical-naming/process-d9df6491/ethanol-local-generated.jpg) |

![Fresh process reopens the same ethanol native file with clean history and C2H6O, CCO and molecular weight in Properties](../images/chemical-naming/process-d9df6491/ethanol-fresh-reopened.jpg)

All three original JPEGs were inspected and copied without cropping,
recompression or annotation. The preview is at 100%; insertion automatically
fits to 250%, retained for generation and fresh reopening. Selection handles
demonstrate the complete generation input. The recovery banner was left
untouched; existing appearance preferences were not changed, and keyboard
drawing was disabled through its native control. These are feature/smoke
examples, not a matched drawing-defect comparison.

The first process was PID 82312; the reviewer observed its kernel executable
path before interaction in the retained tool transcript, without a standalone
PID receipt file. Fresh reopening used PID 85996. The exact
[desktop receipt](../../tests/fixtures/chemical-naming/process-d9df6491/native-desktop-receipt.json)
states that boundary; no additional raw process proof is claimed. The app was
quit after review, leaving no ReShiki process.

This smoke did not repeat the earlier R-lactic, caption, optical-rotation
rejection or manual cancellation cases. Earlier native evidence and separately
compiled tests cover those scopes. No network-denial wrapper surrounded this
GUI process: local controls/provenance were observed, and network-denied tests
are reported separately.

The [supplemental fixture package](../../tests/fixtures/chemical-naming/process-d9df6491/README.md)
contains four original native saves, five accessibility snapshots, raw compact
receipts and a SHA256 manifest, with a portable byte/history/graph check.
This documentation/evidence follow-up changes no production source or payload.

Release caption: Parse supported chemical names into editable structures and
generate local systematic names for supported organic graphs, with specified
stereo checks and one-step Undo.

Under review in [PR #275](https://github.com/Ameyanagi/ReShiki/pull/275), contributed
by @Ameyanagi. Original evidence and documentation are MIT OR Apache-2.0;
upstream OPSIN/dependency notices retain their separate licenses.
