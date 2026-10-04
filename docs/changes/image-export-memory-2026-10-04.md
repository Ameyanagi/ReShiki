# Image and export memory

Stored pictures now discard spare encoded-PNG capacity, reflected pictures reuse
their owned decoded raster, and figure PNG export demultiplies its owned pixmap
in place. SVG conversion shares one parser that consumes its source string and
still loads system fonts afresh on every call. Runtime source decreases by five
physical lines; the opt-in measurement harness adds 211 lines separately.

The baseline is `81ca82101061ecc201545a3cae8d8257f03254d3`. Measurements used
release integration-test builds on macOS. Baseline and candidate used the same
fixture harness and system fonts. These are requested Rust heap observations,
not process RSS, allocator slack, native or GPU memory measurements. Peak means
the maximum additional live requested bytes above the operation's starting
baseline. Allocator-internal realloc temporaries are outside these counters.

## Stored PNG capacity and import tradeoff

The capacity probe retains the normal encoded-image handle, drops its Picture,
and requires `Bytes::try_into_mut()` to recover the uniquely owned storage. It
does not recreate the PNG or fall back to copying. All six controlled runs
reported exactly the following capacities, retained sizes and import peaks:

| Fixture                     | PNG length | Capacity before | Capacity after | Saved capacity and retained bytes | Import requested peak, unchanged |
| --------------------------- | ---------: | --------------: | -------------: | --------------------------------: | -------------------------------: |
| Transparent fixture         |     48,210 |          96,388 |         48,210 |                            48,178 |                        3,064,320 |
| RGBA 2048 × 1536            | 12,585,476 |      25,170,920 |     12,585,476 |                        12,585,444 |                       54,555,688 |
| Solid RGBA 2048 × 1536      |     64,635 |         129,238 |         64,635 |                            64,603 |                       12,865,496 |
| Near-limit RGBA 2000 × 2000 | 16,003,288 |      32,006,544 |     16,003,288 |                        16,003,256 |                       64,414,992 |

The retained Picture has 200 requested bytes above its PNG capacity in both
variants. Savings apply once per distinct stored picture; shared handles and
Undo/clones retain their existing identity and sharing. The associated-alpha
TIFF opacity fixture also reduces retained bytes from 6,294,288 to 3,147,260;
its requested peak remains 12,596,872 bytes.

Shrinking storage adds one successful realloc request and a cumulative allocated
charge equal to PNG length per import. The allocator counts the full new size of
each realloc; this charge does not prove that malloc physically copied the PNG.
The earlier decode/encode peak remains higher than storage compaction.

Initial import timing differences prompted a controlled repeat. Two preserved
test executables were built with identical commands and source trees differing
only in the `Picture::stored` storage line. Six serial processes ran in
off/on/on/off/off/on order. Each process measured nine imports per fixture.
The table gives the median of the three process medians, in milliseconds:

| Fixture                     | Compaction off | Compaction on | Difference | Relative difference |
| --------------------------- | -------------: | ------------: | ---------: | ------------------: |
| Transparent fixture         |       1.769667 |      2.069167 |  +0.299500 |             +16.92% |
| RGBA 2048 × 1536            |      21.567208 |     23.881417 |  +2.314209 |             +10.73% |
| Solid RGBA 2048 × 1536      |       3.901875 |      4.277500 |  +0.375625 |              +9.63% |
| Near-limit RGBA 2000 × 2000 |      38.959583 |     36.917542 |  −2.042041 |              −5.24% |

Individual process medians, in execution order within each variant:

| Fixture                     | Off runs 1, 4, 5 (ms)           | On runs 2, 3, 6 (ms)            |
| --------------------------- | ------------------------------- | ------------------------------- |
| Transparent fixture         | 1.572625, 2.088292, 1.769667    | 2.083833, 2.069167, 1.902792    |
| RGBA 2048 × 1536            | 19.550625, 23.583875, 21.567208 | 23.881417, 27.637167, 22.937167 |
| Solid RGBA 2048 × 1536      | 3.906916, 3.890000, 3.901875    | 3.869625, 5.358250, 4.277500    |
| Near-limit RGBA 2000 × 2000 | 39.701083, 38.673458, 38.959583 | 36.601333, 43.998708, 36.917542 |

Compaction is accepted for its reproducible lasting savings, including roughly
12.6–16.0 MB per large stored picture, with the observed import latency tradeoff
stated above. These timings do not establish a general speedup or an exact
physical-copy cost. The first-reflected-handle control also varied despite its
import and compaction occurring outside that timed operation, demonstrating
process-to-process timing variability. No threshold or extra cache policy is
introduced.

## Reflection and figure export

The full baseline/candidate comparison reports the following requested peaks.
These measure the combined export paths, rather than independent attribution to
each source change. Returned figure/export bytes and cached reflected handles
retain the same requested sizes; reused reflected handles still allocate zero
bytes.

| Operation                                    | Before peak |  After peak | Saved bytes | Reduction |
| -------------------------------------------- | ----------: | ----------: | ----------: | --------: |
| First reflected handle, transparent fixture  |   4,085,760 |   2,325,464 |   1,760,296 |     43.1% |
| First reflected handle, RGBA 2048 × 1536     |  25,165,824 |  12,865,496 |  12,300,328 |     48.9% |
| Reflected export, RGBA 2048 × 1536           |  67,138,600 |  54,555,688 |  12,582,912 |     18.7% |
| Ordinary figure PNG, 1636 × 1481 at 1200 dpi |  20,268,923 |  10,571,835 |   9,697,088 |     47.8% |
| Gallery PNG, 6627 × 5285 at 300 dpi          | 298,024,371 | 157,769,847 | 140,254,524 |     47.1% |
| Picture-heavy PNG, 4858 × 3677 at 1200 dpi   | 416,360,585 | 311,347,089 | 105,013,496 |     25.2% |

The initial five-sample figure medians were ordinary 18.546 → 16.422 ms,
gallery 465.176 → 459.501 ms, and picture-heavy 813.840 → 832.317 ms. They
are workload observations; no broad throughput improvement is claimed.

## Output and behavior verification

Both pre-change capture tests passed candidate verification. They compare exact
PNG/SVG bytes and export receipts, including light/dark themes, colored regular
and bold italic text with CJK fallback, alpha edges, rotated/reflected pictures,
assistant previews, and adaptive gallery DPI. Picture captures also cover narrow
rasters, 16-bit grayscale/color normalization and associated-alpha TIFFs with
EXIF orientations 1–8. The same tiny-skia demultiply operation and PNG writer
settings are used. No font database cache, encoding policy or error mapping was
changed.

The three opt-in measurement tests passed, as did all six controlled storage
runs. The focused normal picture/export/page/font targets passed 23 tests:
`pictures` 9, `figure_exports` 3, `pages` 8, `font_export_103` 2 and
`picture_memory` 1 (five ignored). Windows-native parser callers require the
normal Windows CI checks; this macOS result does not validate Windows rendering
or EMF output. No full-suite or cross-platform result is claimed here.

## Reproduction and evidence

Use separate baseline and candidate checkouts. Copy only the candidate
`tests/picture_memory.rs` harness to the baseline, preserving its runtime source.
Run capture on the baseline with a new empty capture directory, then verify on
the candidate against that directory. Keep the host fonts and profile unchanged.

```sh
RESHIKI_PICTURE_MEMORY_MODE=capture \
RESHIKI_PICTURE_MEMORY_BASELINE_DIR=/tmp/reshiki-images-before \
  cargo test --release --locked --test picture_memory matches_prechange_baseline \
  -- --ignored --nocapture --test-threads=1

RESHIKI_PICTURE_MEMORY_MODE=verify \
RESHIKI_PICTURE_MEMORY_BASELINE_DIR=/tmp/reshiki-images-before \
  cargo test --release --locked --test picture_memory matches_prechange_baseline \
  -- --ignored --nocapture --test-threads=1

cargo test --release --locked --test picture_memory \
  -- --ignored --skip matches_prechange_baseline --nocapture --test-threads=1
```

For the controlled repeat, preserve two executable copies from identical
`cargo test --release --locked --test picture_memory --no-run --message-format=json`
builds. The off variant uses `Bytes::from(bytes)`; the on variant uses
`Bytes::from(bytes.into_boxed_slice())`. Execute each copy serially in the stated
order with `picture_storage_capacity_and_timing --ignored --exact --nocapture
--test-threads=1`, retaining the source and executable hashes with each result.

Recorded evidence is under `/tmp/reshiki-improvements-20261004/evidence`:
`baseline-picture-{bytes,metrics}.log`, `candidate-picture-{bytes,metrics}.log`,
`c03-repeat-{1-off,2-on,3-on,4-off,5-off,6-on}.{log,json}`, and
`picture-compaction-{off,on}-build.json`. The build records show identical changed
file hashes except `src/pictures.rs`; run records identify the executable
actually invoked rather than the source currently checked out at run time.

| Variant | `src/pictures.rs` SHA-256 at build                                 | Executable SHA-256                                                 |
| ------- | ------------------------------------------------------------------ | ------------------------------------------------------------------ |
| Off     | `3ae38671d4dbec7a1c1c01e3d51a3c20f983d806db7b9563703f98693ffecea1` | `88bcef275df6eeda3a60d741b6316ad63b36d8649a35eb950c0b24ccaac1f224` |
| On      | `c00d1905dad3c94902af8b7c7f0e118b21a8fd6142a18a92cd087f3efdd93f07` | `b1204d1cb66bbf70b8a727fad639713bf30309ed5c0fdd99ba45a561929017b3` |
