# Native chemistry allocation refactors — validation, 2026-10-04

Repository path: `docs/changes/native-chemistry-allocations-2026-10-04.md`.

Baseline: `81ca821`; chemistry branch: `refactor/native-chemistry-allocations`. Compilation, unit/reference tests, strict clippy, optimized application-worker verification, and the isolated candidate and matched baseline measurements below passed. Published source provenance is recorded below; GitHub checks identify the reviewed PR head. Tested source hashes identify the uncommitted integrated snapshots below.

## Scope and source budget

R03/R08/R09/R17 and C08/C09/C10/C11/C13 implement ordered shared CDXML defaults, owned import/export transfers, direct aggregate numeric writing, an immutable lazy CDX schema, borrowed InChI serialization, borrowed ranking bond lists, moved cache-miss graphs, and attachment-only ID indexing. A source-only recount against the exact base confirms 21 changed Rust files: production physical source is neutral when test-only declarations/separators are excluded; nonblank/noncomment production source decreases by three lines. Test code and scaffolding account for the entire +773 physical-line increase. There is no scientific-algorithm replacement, source/dependency metadata update, callback-cache redesign, or Python oracle change.

## Validation status

Receipt labels identify local validation runs. The tables, hashes, and reproduction commands in this checked-in note carry the evidence summary; temporary raw receipt files are not required to read it.

| Check                                                                                                                    | Status                                                         | Evidence                                                                                                                                                                                           |
| ------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Worker rustfmt and diff whitespace checks                                                                                | Passed                                                         | Source-only commands in the chemistry worktree.                                                                                                                                                    |
| Full candidate check, all targets/all features                                                                           | Passed                                                         | `evidence/combined-full-check-fixed.json` and `.log`; `cargo check --locked --all-targets --all-features`, return code 0.                                                                          |
| Candidate InChI helper build                                                                                             | Passed: 116.08 seconds                                         | Receipt label `candidate-inchi-helper-fixed`; four build jobs, return code 0; executable SHA-256 `b907e4c22f708830b1eccec085c5c107009527bb6b17dff63c49a592a85b4af6`.                               |
| Full library and binary suite                                                                                            | Passed: 327 library + 493 binary tests, 69 ignored             | `evidence/combined-full-lib-bin-fixed.json` and `.log`; return code 0. Corrected schema field-precedence test passed.                                                                              |
| Mandatory InChI generation, native output, framing/failures, cancellation, deterministic typed heap failure and recovery | Passed: all four tests, 94.49 seconds                          | `evidence/chemistry-reference-remaining.log`; candidate-built helper, mandatory helper discovery.                                                                                                  |
| Native import/response, InChI reader, complete reaction export, ranking/SMILES/MOL and drawing references                | Passed: 35 other tests                                         | `evidence/chemistry-reference-remaining.json` and `.log`; all 39 tests in nine targets passed, return code 0, 369.98 seconds total.                                                                |
| CDXML preparation/molecular and exact CDX encoding                                                                       | Passed: eight tests in three targets                           | Successful target sections in `evidence/chemistry-reference-suite-fixed.log`. Combined reference coverage: 47 normal tests in 12 targets, none skipped.                                            |
| Standalone attachment target                                                                                             | Passed: all 13 tests, none skipped                             | `evidence/final-attachments.json` and `.log`; return code 0, including V3000 ALL/ANY and long continuation records.                                                                                |
| Strict clippy, all targets/all features                                                                                  | Passed                                                         | `evidence/final-combined-clippy-fixed.json` and `.log`; `cargo clippy --locked --all-targets --all-features -- -D warnings`, return code 0.                                                        |
| Optimized application build                                                                                              | Passed: 108.96 seconds                                         | Receipt label `combined-release-app-build`; `cargo build --locked --release --bin reshiki`, return code 0.                                                                                         |
| Optimized application `--inchi-worker`                                                                                   | Passed: 1.05 seconds                                           | Receipt label `combined-release-worker-verification`; framed methane read, protocol/version/status, carbon graph, and total hydrogen valence. Exact binary/verifier hashes and reproduction below. |
| Candidate isolated allocation/time measurements                                                                          | Passed                                                         | Ten initial `chem-*.json`/`.log` pairs plus ranking/refresh repeats, each exact/ignored/release/serial; all return code 0.                                                                         |
| Matched baseline ranking/refresh measurements                                                                            | Passed: allocation figures reproduce in both candidate samples | `chem-ranking-base`, `chem-refresh-base`, `chem-ranking-repeat`, `chem-refresh-repeat`, each `.json` and `.log`; no timing speedup conclusion.                                                     |

Library/binary receipt source diff SHA-256: `3dbf9fa2ca8b14bd9df70ef4c262d77ce424263c4874d149f4aa2fa87e1b5f16`. Remaining reference receipt source diff SHA-256: `6d1ebed1392f5be339e174cca1a78f5756428777bd3422a44363b2f7d6e0d7d5`. These are uncommitted integrated source snapshots, not final revision identifiers. Environment-only setup attempts are excluded from source-regression conclusions. The current MOL test uses `std::slice::from_ref(&attachment)` after the root's test-only clippy correction; the fixed strict-clippy run passed. All ten candidate measurement receipts also record source diff SHA-256 `6d1ebed1392f5be339e174cca1a78f5756428777bd3422a44363b2f7d6e0d7d5`.

## Measured stages and accepted tradeoffs

Every run below uses an isolated optimized test process with one test thread; ranking and refresh additionally have one matched base and two candidate samples. Fixtures are constructed before counter reset; snapshots precede printing. Times are single observed elapsed samples, not statistically established speedups or full application timings. Allocations are successful requested Rust allocation/reallocation counts and bytes. They exclude allocator headers/internal realloc temporaries, native allocations, stack, Python, and RSS. The helper budget separately accounts for actual aligned allocation/header charges and realloc overlap. No native-memory or process-RSS conclusion follows from these counters.

### Ownership, numeric append, and transport

| Measured operation                                   | Allocation count, old → candidate | Requested bytes, old → candidate | Peak additional requested bytes, old → candidate | Observed elapsed, old → candidate |
| ---------------------------------------------------- | --------------------------------- | -------------------------------- | ------------------------------------------------ | --------------------------------- |
| One unique import request transfer                   | 20,007 → 0                        | 6,317,178 → 0                    | 6,317,178 → 0                                    | 945 µs → 166 ns                   |
| One shared import request transfer                   | 20,007 → 20,007                   | 6,317,178 → 6,317,178            | 6,317,178 → 6,317,178                            | 945 µs → 559.292 µs               |
| One terminal reaction-export field transfer          | 20,004 → 0                        | 4,220,015 → 0                    | 4,220,015 → 0                                    | 403.083 µs → 42 ns                |
| 100,000 UINT16 aggregate appends                     | 100,000 → 0                       | 200,000 → 0                      | 2 → 0                                            | 2.498458 ms → 565.083 µs          |
| 20 encodes of a 1,024-atom molecule with coordinates | 680 → 540                         | 24,355,840 → 21,789,280          | 652,616 → 524,288                                | 13.297083 ms → 12.943833 ms       |

Receipts: `evidence/chem-owned`, `chem-numeric`, and `chem-transport`, each `.json` and `.log`. Former operations are modeled in the same test executable; this is not a separately built historical application. The request fixture contains 2 MiB of text, 10,000 atoms, and selected IDs. Ownership rows measure only transfer/clone work, excluding request construction, scheduling, import, and writing. Their cloned outputs remain held at the snapshot; a moved output reuses its fixture allocation. The shared case still clones and its single elapsed sample does not establish an improvement. The numeric destination's preallocated 200,000-byte buffer is already in the live baseline; normal scalar properties still allocate their original exact-width Vec. Transport includes validation and frame encoding but excludes worker generation. Both transport outputs are dropped before snapshot, giving zero additional retained bytes; the observed requested-byte reduction is 2,566,560 over 20 encodes.

### C08: lazy schema retention and repeated construction

| First-use workload                           | Allocation count | Requested bytes | Peak additional requested bytes | Permanently retained after call | Additional permanent bytes on explicit warming | Observed elapsed |
| -------------------------------------------- | ---------------- | --------------- | ------------------------------- | ------------------------------- | ---------------------------------------------- | ---------------- |
| Force schema only                            | 2                | 21,520          | 21,520                          | 21,520                          | 0                                              | 160.292 µs       |
| Valid first decoder read                     | 22               | 22,694          | 22,589                          | 21,520                          | 0                                              | 103.708 µs       |
| First read rejected for unsupported property | 11               | 22,209          | 22,209                          | 21,520                          | 0                                              | 37 µs            |
| Early invalid header                         | 1                | 29              | 29                              | 0                               | 21,520                                         | 250 ns           |

The actual first-read decoder results are dropped before these snapshots. Valid reads and capability failures initialize both immutable maps; an early invalid header does not. An empty property-free drawing may also avoid initialization. Receipts: `evidence/chem-schema`, `chem-schema-valid_read`, `chem-schema-failed_read`, and `chem-schema-invalid_header`, each `.json` and `.log`.

In the default force run, 1,000 former per-call map constructions requested **21,520,000 bytes in 2,000 allocations**, with 21,520 peak additional bytes and no additional retained bytes after the loop. The 1,000 warmed shared lookups requested **0 bytes in 0 allocations**, with zero additional peak or retention; observed elapsed was 5.797708 ms versus 10.833 µs. The other three fresh-process runs showed the same allocation figures (construction 6.628167–6.675375 ms; lookups 12.084–12.209 µs).

The warmed baseline already contains the permanent 21,520-byte schema. Zero warm `peak_extra` does not mean the maps disappeared or the absolute process peak fell by 21,520 bytes. The bounded gate is accepted: repeated codec operations avoid rebuilding the maps, while the first schema-using operation retains **21,520 requested Rust bytes until process exit**, including a supported-frame capability failure. A one-shot process gets no amortized construction benefit and keeps these maps longer than the previous transient builder. These are construction/lookup-stage measurements, not full codec timing.

### C13: attachment-only ID index

The production gate is `attachments.is_empty()`: ordinary MOL and attachment-free V3000/reaction CTAB paths keep the original transient uniqueness set. Only attachment exports retain an ID index through their write. The harness forces both strategies to expose the tradeoff; zero-member indexed rows model a path the production gate avoids. Fixtures and target vectors are prepared before reset.

| Atoms/member lookups | Strategy            | Requested allocations/bytes | Modeled peak/live during serialization | Live after dropping map and buffer | Observed elapsed |
| -------------------- | ------------------- | --------------------------- | -------------------------------------- | ---------------------------------- | ---------------- |
| 32 / 0               | Transient set       | 2 / 3,144                   | 2,560                                  | 0                                  | 7.334 µs         |
| 32 / 0               | Forced index        | 2 / 3,656                   | 3,656                                  | 0                                  | 4.792 µs         |
| 4,096 / 0            | Transient set       | 2 / 401,416                 | 327,680                                | 0                                  | 50.875 µs        |
| 4,096 / 0            | Forced index        | 2 / 466,952                 | 466,952                                | 0                                  | 48.25 µs         |
| 4,096 / 300          | Set + linear lookup | 2 / 401,416                 | 327,680                                | 0                                  | 621.5 µs         |
| 4,096 / 300          | Index + map lookup  | 2 / 466,952                 | 466,952                                | 0                                  | 47.25 µs         |
| 4,096 / 30,000       | Set + linear lookup | 2 / 401,416                 | 327,680                                | 0                                  | 31.057458 ms     |
| 4,096 / 30,000       | Index + map lookup  | 2 / 466,952                 | 466,952                                | 0                                  | 327.709 µs       |

Receipt: `evidence/chem-attachments.json` and `.log`; checksums agree between strategies. A 4,096-atom index retains **139,272 additional requested bytes** beside the 327,680-byte modeled output buffer, raising modeled live/peak bytes to 466,952; cumulative requested bytes rise by 65,536 because the previous set was freed before serialization. No extra live bytes remain after the map and modeled buffer are dropped. The accepted gate pays this temporary attachment-export cost for the observed lookup-stage reduction, without changing the ordinary path or adding persistent retention. There is no size-threshold gate and even small attachment exports use the index.

Timing includes index/set construction, an 80-bytes-per-atom modeled output buffer, and lookup work. It excludes real MOL formatting, wedging, graph preparation, attachment-to-bond search, and deallocation. This does not establish full-MOL latency or actual writer peak-memory gains. No ordinary-path timing gain is claimed.

### C10/C11: matched ranking and refresh allocations

| Workload                            | Count | Allocation count, base → candidate | Requested bytes, base → candidate | Peak additional bytes, base → candidate | Additional retained bytes, base → candidate |
| ----------------------------------- | ----- | ---------------------------------- | --------------------------------- | --------------------------------------- | ------------------------------------------- |
| Rank `CCCCCCCC`                     | 1,000 | 186,000 → 178,000                  | 10,824,000 → 9,704,000            | 4,536 → 4,536                           | 0 → 0                                       |
| Rank `C[C@H]1CCC[C@@H](C)C1`        | 1,000 | 122,000 → 122,000                  | 8,672,000 → 8,672,000             | 4,612 → 4,612                           | 0 → 0                                       |
| Rank `C1[C@H]2CC[C@@H]1CC2`         | 1,000 | 211,000 → 197,000                  | 13,578,000 → 11,018,000           | 4,298 → 4,138                           | 0 → 0                                       |
| Refresh 1,024 atoms, cache miss     | 100   | 5,600 → 5,400                      | 36,424,900 → 33,150,500           | 211,886 → 188,357                       | 1,023 → 1,023                               |
| Refresh 1,024 atoms, matching cache | 100   | 5,800 → 5,800                      | 37,346,400 → 37,346,400           | 221,101 → 221,101                       | 1,023 → 1,023                               |

Allocation/peak/retention figures matched in both candidate runs. Ranking removed 8,000 allocations/1,120,000 requested bytes for the acyclic fixture and 14,000 allocations/2,560,000 bytes for the bicyclic fixture per 1,000 ranks; one chiral-ring fixture was unchanged. Cache-miss refresh removed 200 allocations/3,274,400 requested bytes across 100 calls; cache-hit figures were unchanged. No new retained allocation appears in these matched measurements.

| Workload                | One base elapsed sample | Candidate first / repeated elapsed samples |
| ----------------------- | ----------------------- | ------------------------------------------ |
| Acyclic ranking         | 16.725875 ms            | 9.444791 / 14.8805 ms                      |
| One chiral ring ranking | 4.996625 ms             | 6.065291 / 5.028834 ms                     |
| Bicyclic ranking        | 8.753333 ms             | 10.362583 / 8.289084 ms                    |
| Cache-miss refresh      | 7.482208 ms             | 8.533667 / 7.388291 ms                     |
| Cache-hit refresh       | 8.399541 ms             | 9.222292 / 7.976375 ms                     |

Timing varies across these one-base/two-candidate samples and establishes no robust speedup. Receipts: `evidence/chem-ranking`, `chem-refresh`, `chem-ranking-base`, `chem-refresh-base`, `chem-ranking-repeat`, and `chem-refresh-repeat`, each `.json` and `.log`, all return code 0. The baseline was `81ca821` with only test scaffolding: these two ignored functions plus the separate template evidence test module. A source check found no baseline production diff in `ranking/compare.rs` or `inchi/kernel.rs`, and the two benchmark-file diffs exactly matched the transplant patch. Baseline diff SHA-256: `8471e1de2ee90b484c5b70ee6af092b906fcc32a50bfa78300a6bc4864efb134`; candidate diff SHA-256: `6d1ebed1392f5be339e174cca1a78f5756428777bd3422a44363b2f7d6e0d7d5`.

The refresh method does not save its returned State into Toolkit, so the uncached arm stays a miss throughout the loop. The 1,023 retained requested bytes belong to the fixture Toolkit's per-bond `unspecified` vector and last until that Toolkit is dropped; this storage already existed in the baseline. Fixtures and the cache-hit State are constructed before reset. These are in-process callback/ranking allocation measurements, excluding helper allocator header/padding and native work; the separate candidate-built helper suite establishes deterministic budget failures and recovery.

## Commands and provenance

The validation host was macOS arm64 (`aarch64-apple-darwin`); the helper build record reports `rustc 1.99.0 (b940084d7 2026-09-28)`, LLVM 23.1.1. Cargo commands used the locked dependencies. InChI is **1.07.5**, from `cosmolkit-inchi` 0.3.0 pinned to revision `3a437849dcd28319b1a3cdf02d7897a7ee200cb2`. CPython **3.12.12** drove helper building, optional reference tests, and release-worker verification; the repository `.python-version` selects 3.12 and `pyproject.toml` supports `>=3.11,<3.14`. The locked optional reference environment uses RDKit **2026.3.6** and uv `>=0.12.3`. Python serves the development verifier/oracle; the compiled application handles chemistry in its native worker.

Reproduce from the candidate repository checkout. `uv sync --locked --python 3.12.12` prepares the optional reference `.venv`; the Python commands below use that interpreter on Unix (`.venv/Scripts/python.exe` on Windows). The native worker verifier imports only Python's standard library and repository scripts, so RDKit is not needed for that specific check. Build the candidate helper and fault stub in the default `artifacts/inchi-helper` directory; reference tests discover them there. For application-facing tests set `RESHIKI_INCHI_HELPER` to the helper executable file, never its directory. `RESHIKI_REQUIRE_INCHI_HELPER=1` prevents silent helper-test skips.

```sh
uv sync --locked --python 3.12.12
.venv/bin/python scripts/build_inchi_helper.py --jobs 4
export RESHIKI_INCHI_HELPER="$PWD/artifacts/inchi-helper/reshiki-inchi-helper"
cargo check --locked --all-targets --all-features
cargo fmt --all -- --check
cargo test --locked --lib --bin reshiki -- --test-threads=1
RESHIKI_REQUIRE_INCHI_HELPER=1 cargo test --locked --features rdkit-reference --test inchi_generator --test inchi_reader --test native_import --test native_response
cargo test --locked --features rdkit-reference --test cdxml_preparation --test cdxml_molecular --test cdx_codec --test reaction_output --test ranking --test smiles_write --test molfile --test molfile_drawing
cargo test --locked --test attachments
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release --bin reshiki
```

### Optimized application-worker evidence and reproduction

The actual optimized application executable passed `build_release.verify_inchi_worker(binary, INCHI_VERSION)`. The verifier sends methane `InChI=1S/CH4/h1H4` with a 64 MiB heap budget to that application's `--inchi-worker` entry point. It checks the `RSHINCHI` response header, protocol 3, reserved bytes, response size/frame length, InChI version 1.07.5, native status 0, one neutral carbon atom, no bonds, and explicit plus implicit hydrogen count 4. The recorded process returned successfully in 1.05 seconds after the optimized build completed in 108.96 seconds.

| Recorded artifact                                      | SHA-256                                                            |
| ------------------------------------------------------ | ------------------------------------------------------------------ |
| Optimized application executable                       | `4468f388d795b783504a156047bb9bce0e89c9ef67588454a5ddda7fd2a83366` |
| Checked-in verifier `scripts/build_release.py`         | `59bb5db9b0cb5e7178f4f70025d80590ae76cb224fda0d52dfc4c542364dc011` |
| Integrated source diff used for build and verification | `6d1ebed1392f5be339e174cca1a78f5756428777bd3422a44363b2f7d6e0d7d5` |

This reproduction discovers Cargo's configured target directory instead of assuming a temporary build path, imports the checked-in verifier, and prints the resulting executable hash. Hashes identify this recorded build; a rebuilt executable may differ with compiler/platform/build inputs.

```sh
.venv/bin/python - <<'PY'
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path("scripts").resolve()))
import build_release
from build_inchi_helper import INCHI_VERSION

metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
    text=True,
))
name = "reshiki.exe" if os.name == "nt" else "reshiki"
binary = (Path(metadata["target_directory"]) / "release" / name).resolve(strict=True)
build_release.verify_inchi_worker(binary, INCHI_VERSION)
print(json.dumps({
    "result": "passed",
    "inchi_version": INCHI_VERSION,
    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
}, sort_keys=True))
PY
```

Run each ignored test in an isolated fresh process, release profile, and one test thread. Snapshot counters precede printing. The receipts record commands, revisions/source hashes, overrides, and raw logs; optimized release profile and one test thread are explicit. Ownership/schema/numeric/transport/attachment rows above are single samples; ranking/refresh have one base and two candidate samples. No timing speedup conclusion is drawn. Match architecture/compiler/features and alternate/repeat more runs if timing attribution is later required. For a matched base measurement, copy only `measure_ring_chiral_ranking` from `src/chemistry/ranking/tests.rs` and `measure_toolkit_refresh` from `src/chemistry/inchi/kernel/tests.rs` into their existing test modules in an isolated `81ca821` checkout. Keep its production ranking/kernel implementations. The baseline already has the same test-only measured allocator and private helpers; no production allocator or feature change is needed.

```sh
cargo test --locked --release --lib engine::tests::measure_owned_request_transfers -- --ignored --exact --test-threads=1 --nocapture
cargo test --locked --release --lib exchange::tests::measure_schema_retention -- --ignored --exact --test-threads=1 --nocapture
RESHIKI_SCHEMA_FIRST_USE=valid_read cargo test --locked --release --lib exchange::tests::measure_schema_retention -- --ignored --exact --test-threads=1 --nocapture
RESHIKI_SCHEMA_FIRST_USE=failed_read cargo test --locked --release --lib exchange::tests::measure_schema_retention -- --ignored --exact --test-threads=1 --nocapture
RESHIKI_SCHEMA_FIRST_USE=invalid_header cargo test --locked --release --lib exchange::tests::measure_schema_retention -- --ignored --exact --test-threads=1 --nocapture
cargo test --locked --release --lib exchange::values::tests::measure_numeric_append -- --ignored --exact --test-threads=1 --nocapture
cargo test --locked --release --lib chemistry::inchi::generator::transport::tests::measure_borrowed_generation_transport -- --ignored --exact --test-threads=1 --nocapture
cargo test --locked --release --lib chemistry::ranking::tests::measure_ring_chiral_ranking -- --ignored --exact --test-threads=1 --nocapture
cargo test --locked --release --lib chemistry::inchi::kernel::tests::measure_toolkit_refresh -- --ignored --exact --test-threads=1 --nocapture
cargo test --locked --release --lib chemistry::molfile::tests::measure_attachment_index_tradeoff -- --ignored --exact --test-threads=1 --nocapture
```

Every work-budget charge, validation order, protocol/error variant/message, heap guard, and cancellation boundary remains. Removing real helper allocations can lower honest `used`/`requested` diagnostics or move an exhaustion threshold; do not recreate allocations or use virtual charges to freeze historical values. Validate deterministic same-operation failures and subsequent recovery with the candidate executable. Scientific outputs, exact frame/property bytes, stable IDs, and input snapshots must remain unchanged.

## Deferred work

Callback caching remains deferred. The pinned dependency can change graph fields, cached valence, stereo, and cleanup state; unchanged counts or assumed callback sequencing do not establish safe reuse. No invalidation guard was weakened. A later proposal needs a complete mutation/invalidation proof and a separate LOC/retention estimate.

## Published source

[PR #141](https://github.com/Ameyanagi/ReShiki/pull/141) publishes chemistry source commit `ee7ebed62043bca7b9b53e23cacd25de1dabc0d2` on base `aa4f85c595cff7771570c2f9d943948f3df1a961`. Its binary Git source patch (`git diff --binary aa4f85c595cff7771570c2f9d943948f3df1a961 ee7ebed62043bca7b9b53e23cacd25de1dabc0d2 -- src`) has SHA-256 `aa965263934b4d816851ea340bf27a4d44e0099fa0b581ec1f1244f720a2220a`. The commit rebase changes no chemistry source bytes from the tested candidate; other merged refactors were also present in the integrated validation snapshot. Subsequent release-note/provenance edits are documentation only. GitHub checks and the PR record identify the final reviewed head.
