# Ubuntu 22.04 saved depiction expectations

Set `RESHIKI_TEST_LINUX_GOLDENS=ubuntu-22.04-x64` when running the saved
depiction suites on Ubuntu 22.04 x64. The reader selects the four Ubuntu
geometry, rings, attachment and templates captures. An unset variable retains
the existing Arch captures; unknown values and non-Linux-x64 targets fail.
The selector does not inspect Rust results or change numerical comparisons.
The remaining four stages use their existing captures.

All 9,539 input records in these four captures equal the corresponding Arch
records after removing only `expected`. Case order, binary64 inputs, molecule
pickles and state metadata are unchanged. Expected results came from the
original native observers, never the Rust implementation. Geometry, rings
and attachment retain the independent Ubuntu captures from the
[paired-trigonometry audit](depict_linux_math.md); templates were captured by
replaying the existing corpus through the original C++ observer on the same
container. The template replay contains all 1,921 cases.

Each gzip file retains its original provenance header, including RDKit
2026.03.6, source commit `0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`, source
and library hashes, and observer hash. The template header also contains its
GCC 11.4.0 / Boost 1.85 build manifest. The reported Arch kernel belongs to
the container host; the userland is Ubuntu 22.04.5 with glibc
`2.35-0ubuntu3.14`. Image digest and `libm` hash are recorded in the linked
audit. No native observer or source checkout is needed for saved replay;
the existing Python fixture readers still need the pinned test environment.

| Stage      | Cases | Gzip SHA-256                                                       | Uncompressed JSONL SHA-256                                         |
| ---------- | ----: | ------------------------------------------------------------------ | ------------------------------------------------------------------ |
| geometry   | 5,310 | `6f2da86aaaf8bb03c5c95539929af9743ca533a71eec64eeb1b5eebf947de172` | `48e79a8f7069acbcaf6509fb0c91f9fa4336c08cf1813ed4d43ce6293cacf005` |
| rings      | 1,338 | `ffcf5b04b0006079b37b9aa20d8ec724195e39eee35cd894a0987b0bab231b79` | `f8b56819b621477aac9b72e0542f8bf3c24790479e7461f62e3a03754cbbf345` |
| attachment |   970 | `2d390c39d86fd927d28149a3e90ca6f42a1d4c4123609aee1b3297182393d080` | `edb70cb3b36d3152d1df9943b7f0ead5197526190d8e9f009e44b4adadd7f694` |
| templates  | 1,921 | `59a33777c4789b1a47a8244aaf5cd3df374ae4f7b71f43460a9f630c84e7c099` | `15bf119fca7d90e9dc8352559990d7acd9c89659005546f6240bb254d2b75257` |

For saved replay, leave `RESHIKI_DEPICT_ORACLE`, the stage-specific
`RESHIKI_DEPICT_*_ORACLE` variables and `DEPICT_EXPANSION_ORACLE` unset.
Run the eight targets `depict_geometry`, `depict_rings`, `depict_attachment`,
`depict_seeds`, `depict_templates`, `depict_collision`, `depict_expansion`
and `depict_finalize` with `--features rdkit-reference`. Their existing case
counts, topology checks, exact same-platform scalar checks and foreign-platform
audits remain in place. Live observer environment variables still enable fresh
native replay for scheduled, manual and release validation. A changed system
math result must fail the exact saved expectation; it does not select another
fixture automatically.
