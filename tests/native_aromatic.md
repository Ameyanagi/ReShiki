# Complete aromatic response goldens

`complete_aromatic_responses_match_goldens` is the Python-free fast entry point.
It reads checked-in gzip JSONL in Rust and runs the native aromatic backend with
the pinned InChI helper. It never invokes Python, invokes the capture tool, or
updates expected results. The corpus remains all 13,425 original cases, including
whole/partial selections, fused rings, both toggle directions, stale presentation
metadata, and invalid abbreviations.

`complete_aromatic_responses_match_original_worker` retains the live oracle. Both
paths bound concurrent cases to the smaller of four workers and the available CPU
count. Live workers each own a separate `PythonEngine`; cloning a single engine
would share its locked child and serialize requests. Case failures retain their
names and are reported in corpus order. The corpus producer is explicitly killed
and reaped on early failure; worker/helper children retain their existing
kill-on-drop behavior.

All existing comparisons are retained: complete serialized responses, exact error
strings, immutable input snapshots, and more than 5,000 accepted and 500 rejected
cases. The existing relative `1e-12` rule for mass, exact mass, logP and TPSA is
unchanged. Captured case counts, separate preflight counts, request round-trips,
source identities, and record hashes are also checked.

## Reference boundary and provenance

The original live `PythonEngine` validates the document in Rust before invoking
Python. Some invalid abbreviation requests stop there and have a different error
from `engine.worker.handle`. Fixtures therefore keep these as distinct fields:

- `request` is the exact `serde_json::to_value(Request)` used by the live bridge,
  including document defaults and f32 coordinates.
- `preflight_error` is a fixed diagnostic captured from that pinned Rust boundary,
  or null. Replay does not call runtime validation to construct its expectation.
- `expected` always contains the independent original Python worker response or
  error, including for requests rejected at the preceding boundary.

The request-serialization step never calls `native_aromatic::execute` and never
produces a molecular worker response. Expected worker results come exclusively
from `engine.worker.handle` with pinned RDKit 2026.03.6, upstream source commit
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`.

Every fixture header records platform/Python/native build identities, hashes of
native libraries, the NCI input data, Python sources and serializer sources, and
all counts, including 395 fixed preflight rejections. Historical serializer hashes describe capture provenance; replay
checks each complete request against the current schema instead of regenerating
expected outputs after Rust implementation changes. Python reference-source
hashes and the complete record stream checksum are checked on replay.

There are separate macOS ARM64, Linux x64 and Windows x64 captures. Native corpus
layout makes 2,952 macOS/Linux requests and 2,909 macOS/Windows requests differ.
For identical requests, six macOS/Linux and four macOS/Windows worker outputs
differ only in exact-mass rounding, within the already-existing numeric comparison.
Their coordinates are retained; no realignment or cross-platform replacement is
performed. Linux ARM and Windows ARM consume the captured requests selected by
OS, so they do not regenerate inputs with different native layout arithmetic.

## Explicit refresh

Use the pinned development environment and build the InChI helper separately for
replay. Capturing worker responses does not use the Rust helper. For each source
platform, run these three explicit steps from the repository root:

```sh
uv run python tests/native_aromatic_reference.py --corpus --output artifacts/aromatic-raw.json.gz
CARGO_BUILD_JOBS=4 uv run python tests/native_aromatic_reference.py --serialize artifacts/aromatic-raw.json.gz --output artifacts/aromatic-requests.json.gz
uv run python tests/native_aromatic_reference.py --requests artifacts/aromatic-requests.json.gz --output tests/fixtures/native-aromatic-PLATFORM.json.gz
```

The middle step calls only the ignored `serialize_aromatic_requests` test, using
a fresh temporary output path. It requires exactly one successful test, the full
count sentinel, and all output records. A Cargo filter matching zero tests cannot
reuse an old output. Request serialization may run on a different host with the
same source/schema; the native input platform and serializer source hashes remain
in the output provenance.

Review refreshed fixtures explicitly, then run:

```sh
RESHIKI_REQUIRE_INCHI_HELPER=1 cargo test --locked --features rdkit-reference --test native_aromatic complete_aromatic_responses_match_goldens -- --exact --nocapture
```

The live comparison remains a separate scheduled/manual/release check. This is
incremental golden coverage for complete aromatic responses, not a claim that
all reference suites have been converted.

## Validation checkpoint

Linux x64 replay passed with the checkout's `.venv` removed and Python overrides
pointing to a missing interpreter: 9,960 complete responses, 3,070 independent
worker rejections, and 395 fixed preflight rejections in 53.64 seconds. The bounded
live mode passed the same 13,425 cases in 292.10 seconds. Both used four workers;
these are local measurements, not a controlled comparison with the earlier hosted
x64/ARM timings. The two smaller aromatic response/validation tests also pass.

Each platform fixture contains the same full case count and occupies about
5.6 MB compressed. macOS and Windows native captures were generated independently;
Rust replay on those platforms is left to the hosted cross-platform checks.
