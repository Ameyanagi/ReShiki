# Memory footprint changes

Three small runtime changes reduce temporary allocations while preserving drawing
and worker output. The baseline is `cab04d87e271f2e8a0c1b2bc15a27306b6200d48`.
Measurements used Apple M4 arm64, macOS 26.5.1, Rust 1.99, Python 3.12.12 and
RDKit 2026.03.6. Numeric and Rust-worker measurements used debug test builds;
picture measurements used release builds. These are workload observations, not
application-wide memory estimates or timing benchmarks.

- Numeric transforms mutate one candidate across the requested fields, preserving
  their order, validation errors, draft state and document-history behavior.
- Picture paths consume owned decoded buffers instead of copying RGBA buffers.
  Grayscale conversion, opacity, EXIF orientation, reflection and encoding remain
  unchanged; the reflected handle cache still reuses its existing handle.
- The Rust reference exchange releases its request value after serialization and
  its encoded bytes after flushing, and moves the result out of the response.
  Python releases the request, response and input line after its successful flush.

Runtime code has a net line-count change of zero: numeric −2, Rust/Python workers
+2, pictures 0. Development instrumentation and tests add code separately. The
optional `allocation-metrics` feature wraps the existing allocators only in test
binaries. Production heap budgeting and application unsafe-code prohibitions are
unchanged; no third-party dependency was added.

## Requested allocation observations

All numbers in this table are **bytes**, original → candidate. Peak means the
maximum additional live requested Rust bytes above the workload's starting
baseline. Allocated bytes sum successful requested sizes, including the full new
size on each successful realloc; they are not physical bytes allocated by malloc.
Fixture construction and warmup are excluded. Numeric rows cover 30 candidates,
the Rust worker covers 30 exchanges with 1 MiB payloads, and picture rows cover
one operation. Native, GPU, child-process, allocator-header and RSS bytes are
outside these counters.

| Workload                                  |   Peak additional bytes |           Allocated bytes | Successful allocation/realloc calls |
| ----------------------------------------- | ----------------------: | ------------------------: | ----------------------------------: |
| Numeric, whole selection                  |   2,162,815 → 1,531,018 | 191,503,380 → 115,687,740 |                   847,740 → 367,500 |
| Numeric, boundary stereo                  |   2,006,469 → 1,424,704 |  162,198,480 → 92,769,180 |                   803,100 → 367,620 |
| Picture 2048×1536, ordinary import        | 67,138,600 → 54,555,688 | 124,718,848 → 112,135,936 |                             52 → 51 |
| Picture 2048×1536, opacity import         | 79,721,512 → 54,555,688 | 137,301,760 → 112,135,936 |                             53 → 51 |
| Picture 2048×1536, reflected export       | 79,721,512 → 67,138,600 | 149,887,236 → 137,304,324 |                             54 → 53 |
| Picture 2048×1536, first reflected handle | 37,748,736 → 25,165,824 |   38,162,416 → 25,579,504 |                             11 → 10 |
| Picture, reused reflected handle          |                   0 → 0 |                     0 → 0 |                               0 → 0 |
| Associated TIFF 1024×768, opacity import  | 18,888,328 → 12,596,872 |   41,575,744 → 32,138,560 |                             90 → 87 |
| Rust reference worker exchange            |   7,347,480 → 3,148,568 | 314,556,666 → 283,040,440 |                       1,620 → 1,291 |

Picture retained sizes are identical across all measured operations. The held
whole-selection numeric candidate remains 631,797 bytes. The boundary candidate
changes from 577,515 to 581,515 bytes: clearing stereo vectors retains 4,000 bytes
of capacity that the old deep-clone sequence compacted. Dropping either numeric
candidate returns its additional live requested bytes to zero. Rust-worker
retained observations were 0 and 64 bytes; this small difference does not support
a leak-freedom claim or a general retained-memory reduction.

Python used an AST-extracted production `main` with a simulated 8 MiB request and
separate 8 MiB result. Prebuilt input lines were outside tracing and serialized
output was discarded. **Python tracemalloc bytes**, original → candidate:

| Observation                  |            Traced bytes |
| ---------------------------- | ----------------------: |
| Waiting for the next request |      16,778,970 → 1,552 |
| Next small handler begins    |       8,390,892 → 1,723 |
| Maximum traced allocation    | 35,653,894 → 35,653,894 |

This fixture proves shorter Python packet ownership between requests. Its peak
remains serialization dominated. It does not measure actual RDKit heap use or
process RSS.

## Behavior verification

- All 44 picture byte captures match, including narrow rasters, transparency,
  16-bit grayscale/color and associated-alpha TIFFs with eight EXIF orientations.
- All nine numeric transcript cases match exactly: 292,670 bytes, SHA-256
  `b91a7a581c793a900535870a15f2fd094bdcc34b2d10e2c8e0fa98d9cb0418d4`.
- Real Python worker stdout matches exactly: 4,271 bytes, SHA-256
  `50a1cabe862d6848f220e53e003f20c967a44027d870371d4b36aa6d1d7cd762`.
  Both streams returned zero with empty stderr, covering malformed requests,
  protocol rejection, successful imports and an unknown-operation error.
- Five normal Rust protocol tests plus the ignored allocation workload passed.
  Python recovery/framing/ownership tests passed, including ownership through flush.
- The direct allocator regression passed zeroing, alignment, data preservation,
  resize, baseline reset and failed-request accounting.
- `cargo test --locked --no-default-features` passed **1,206 tests across 74 suites**,
  with 61 ignored, in 217.61 seconds. This recorded memory-run tree excludes the
  later ordinary Arrange regression; that test passed separately on both original
  and candidate implementations and is not added to the 1,206 count.
- The normal release application build passed; its dependency graph excludes the
  development allocation-metrics feature.
- [Three-platform worker captures](https://github.com/Ameyanagi/ReShiki/actions/runs/37198747899)
  refreshed provenance after the worker source edit. All 13,425 request/response
  records on each platform remain byte-identical; all 28 source fingerprints and
  the original input hashes were verified. Strict fixture guards remain enabled.

## Reproduction

Use separate original and candidate checkouts and target directories. Apply the
same development harness/allocator instrumentation to the baseline without its
runtime edits. Run this block first there with `phase=baseline`, `mode=capture`,
then in the candidate with `phase=candidate`, `mode=verify`. Capture originals
before verifying candidates; do not overwrite the baseline with candidate output.

```sh
audit=/tmp/reshiki-memory-recheck
phase=baseline
mode=capture
mkdir -p "$audit"
export CARGO_TARGET_DIR="$audit/target-$phase"
RESHIKI_NUMERIC_TRANSFORM_CAPTURE="$audit/numeric-$phase.json" \
  cargo test --locked --no-default-features --bin reshiki \
  app::numeric_transforms::memory_tests::numeric_transform_memory_characterization \
  -- --ignored --exact --nocapture --test-threads=1
cargo test --locked --no-default-features --bin reshiki \
  app::numeric_transforms::memory_tests::numeric_transform_candidate_memory_workload \
  -- --ignored --exact --nocapture --test-threads=1
RESHIKI_PICTURE_MEMORY_MODE="$mode" RESHIKI_PICTURE_MEMORY_BASELINE_DIR="$audit/pictures" \
  cargo test --release --locked --no-default-features --test picture_memory \
  -- --include-ignored --nocapture --test-threads=1
cargo test --locked --lib --features rdkit-reference engine::reference::tests::protocol \
  -- --include-ignored --nocapture --test-threads=1
```

Compare numeric captures with `cmp "$audit/numeric-baseline.json" "$audit/numeric-candidate.json"`.
Run Python regressions with `uv run --locked python -m unittest tests.test_worker_protocol`.

Raw logs, run commands, source hashes, transcripts, 44 picture artifacts and the
Python capture script are in `/tmp/reshiki-memory-audit-20261004`. The paired logs
are `numeric-memory-{baseline,candidate}.log`, `picture-{baseline,candidate}.log`,
`worker-rust-{baseline,candidate}.log` and `worker-python-results.md`; normal-suite
results are in `memory-normal-tests.log` and `memory-normal-tests-run.json`.
