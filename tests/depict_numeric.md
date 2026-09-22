The depiction arithmetic policy reproduces the pinned native ABI's operations.
Linux x86_64 keeps separate multiply/add operations. macOS ARM64 contracts only
independently verified source sites and uses the native paired sine/cosine call.
It does not change atom/ring iteration, thresholds, branch comparisons, matrix
product order, centering, signed zeros or output projection. No tolerance,
coordinate alignment or frozen difference count is an acceptance condition.
This is a library checkpoint; application layout dispatch remains separate.

The four dedicated suites cover 8,609 native requests and 484,252 finite fragment
scalar values, plus 9,478 ligand-angle queries. Linux x86_64 and macOS ARM64
fixture comparisons require identical f64 bits and independently calculated
f32 drawing projection bits. Mac ordinary debug and release both pass all 13
Rust tests. Cross-platform fixtures remain raw audits and are not evidence of
same-platform parity. Optional live native replay always requires exact bits.
Windows x64 passes all 13 tests against its own strict fixtures, with no unequal
scalar or projected coordinates. Windows ARM64 Rust against an x64 emulated
reference requires its own CI verification and is not established by the
physical x64 capture host.

## Verified native arithmetic

Native source is RDKit 2026.03.6 commit
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. The private arithmetic helpers have no
mutable state. They select explicit `f64::mul_add` only on macOS ARM64; elsewhere
they retain separate source-order products. The second product in dot/cross
expressions rounds before accumulation of the first. Covariance and full 3×3
matrix multiplication retain each source accumulation, including zero terms.

The following disassembly addresses are relative to the pinned Mac dylibs:

| Native operation     | Evidence                                                      | Rust use                                         |
| -------------------- | ------------------------------------------------------------- | ------------------------------------------------ |
| Point2D length       | RDGeometryLib `cc40` fmul, `cc44` fmadd                       | squared length and normalization                 |
| TransformPoint       | `e130` fmul, `e134` fmadd, `e13c` separate translation        | transform application                            |
| Transform alignment  | `eb20`/`eb2c` norms, `eb44` dot, `eb70` fnmul, `eb74` fmadd   | length, normalized dot and rotation sign         |
| SquareMatrix product | `e4e0`/`e644` fmadd                                           | each of the three ordered products               |
| Canonical covariance | Depictor `dd94` fmadd, `dd98` fmla                            | xx, xy, yy accumulations                         |
| Eigenvectors         | `de00` discriminant fmadd; `de18`/`de64` norm                 | source threshold branches unchanged              |
| Ring mirroring       | `20b70` fnmul, `20b74`/`20b7c`/`20b9c`/`20ba8` fmadd          | mirror denominator, norm and matrix              |
| Attachment           | `29a48` fnmul, `29a4c` fmadd; norms `29d94`, `2a5ac`, `2ad08` | rotation sign, incoming normal and normalization |
| Coordinate seeds     | `367e4`/`36810` norms, `36838` dot; `23e28` cross             | attachment angles and neighbor ordering          |
| Paired sin/cos       | Depictor ring `3a954`, RDGeometryLib alignment `ebb0`         | ring phases and transforms                       |

Mac Depictor SHA-256 is
`162153ed0c2b08333666def744c11799ecf0f09690ec07d582ceaa084e2b5f78`;
RDGeometryLib is
`d092c7ae3b51f82dfdf788afd9585af73883d61f83ba45469356b324e1d052f7`.
The source files and all other library hashes are in each fixture's provenance.
No foreign libm implementation was copied.

Native contraction affects branches: the opposite-vector ring witness documented
in `depict_rings.md` has an unfused zero cross product but native Mac
`-5.497719261874257e-17`. The verified contraction reproduces the native rotation
sign and all subsequent coordinates. Seed normals and attachment-angle differences
likewise disappear when their precise native norm/dot operations are reproduced.

## Paired libm in ordinary debug builds

The pinned Mac native library calls `__sincos_stret`; separate libc `sin`/`cos`
can return different bits. One ring witness is `ring/29/1e-10/0`, scalar 7:
separate calls yield `2.79864830450475026e-10`, while the native pair yields
`2.79864830450474975e-10`. Calling Rust `sin_cos()` at optimization level zero
still emits separate calls, so merely grouping the source expressions is
insufficient.

`reshiki-depict-math` is a tiny local crate with no dependencies and
`#![forbid(unsafe_code)]`. Its non-inlined wrapper calls `f64::sin_cos()`.
Package-scoped dev/test optimization level 1 allows the compiler to select the
paired native call; the application's own optimization settings remain unchanged.
The release profile requires no override. `nm`/`otool` of the debug wrapper shows
only the `___sincos_stret` import and a single call. Verified Rust compiler:
1.95.0, commit `59807616e1fa2540724bfbac14d7976d7e4a3860`, LLVM 22.1.2,
`aarch64-apple-darwin`. Compiler or library changes must pass the same strict
fixtures; no code-generation guarantee is inferred for an untested toolchain.

```sh
CC=/usr/bin/clang CXX=/usr/bin/clang++ CARGO_BUILD_JOBS=2 \
  cargo +1.95.0 test --locked --test depict_geometry --test depict_rings \
  --test depict_attachment --test depict_seeds -- --nocapture
# Repeat with --release. Linux validation uses CARGO_BUILD_JOBS=4.
```

## Independent native captures

Both builders require Boost headers matching `rdBase.boostVersion` (1.85 for
these wheels). Boost 1.91 is not a compatible substitute for header-side graph
iteration: it caused observer crashes on Windows. These were reference ABI
errors, not native chemistry cases. Rebuilding all four helpers with 1.85 on
Linux and Mac reproduced every existing capture exactly, excluding provenance.
The official Boost 1.85.0 source archive SHA-256 is
`7009fe1faa1697476bdc7027703a2badb84e849b7b0baad5086b087b971f8617`.

Mac/Linux generation uses `tests/build_depict_geometry_oracle.py --component`
with `--rdkit-source`, `--boost-include`, `--replay` and `--output`. The original
C++ methods produce expected outputs. Python preserves input bits and serializes
results; Rust does not participate. An isolated source/include tree is sufficient.

Windows generation uses the pinned x64 wheel's Python in an x64 MSVC developer
prompt, for each of `geometry`, `rings`, `attachment`, `seeds`:

```bat
python tests\build_depict_windows_oracle.py --component seeds ^
  --rdkit-source F:\isolated\rdkit --boost-include F:\isolated\boost185
```

The builder extracts import libraries from the actual wheel DLL exports and
compiles one job with `/std:c++20 /O2 /MD /EHsc /DWIN32 /DBOOST_ALL_NO_LIB`.
The capture used MSVC 19.43.34809.0 (toolset 14.43.34808). It records compiler
arguments/log hash, original/reference/adapter/header hashes,
library hashes and executable hash. The physical capture host is x64 Windows 11,
AMD Ryzen 9 7940HS; neither the helper nor Python was ARM64/emulated.
Crash-dialog suppression is scoped to the generator process and restored.
No SDK, system configuration or runtime application dependency is installed.

Windows does not export the four DepictorLocal seed helpers. The builder copies
bytes `[1068,7003)` verbatim from the pinned `RDDepictor.cpp`, containing
`getRankedAtomNeighbors`, `embedSquarePlanar`, `embedTBP`, `embedOctahedral` and
associated constants. Only includes and namespace wrappers are added. The copied
functions call the original wheel's EmbeddedFrag and Chirality methods. Original
source SHA is `f1ae9e705405d5254575c595c09c5d2ee240d4d2abf05209dd42dad5ae99b31f`;
verbatim body SHA is
`0be8a251293bea662453dbd671d7dc1ca39b5b0b2fc12f2cced897821b5e3f39`; generated adapter
SHA is `37dab5b6ea2a26e86b3e054c580979135ed282bc50e001288f67143a7fcca9c3`.
The existing single-line private→public observation header patch changes no
member layout or function body. All source remains covered by `licenses/rdkit`.

Windows fixture compressed / uncompressed SHA-256:

| Fixture    | Compressed                                                         | JSONL                                                              |
| ---------- | ------------------------------------------------------------------ | ------------------------------------------------------------------ |
| geometry   | `d6006199a68e7a6e894bacefeb67d5828f19ee4463560f8d16a8795795319d4b` | `0637e7150477a5eac85207092a031a165c0833e175d1b449434accbaa3a06712` |
| rings      | `950dfc2ec3a399b79b28f86b1361490b25c0a8ec9842c644d6aed94786742779` | `273bc1492d89441bf9d94cc09cb59a90b46542e3912b7f8138eb29c303c9369c` |
| attachment | `6b859b518551339ea0902c874b3397395a40ad60961048e6bc24f0dea2d52ca7` | `2271965e1f8ce996ce6a936c25d220a4364494dd9943c0e582db5de7e50c187c` |
| seeds      | `c9d05d51f96af38d6bdf5bc5dd058012a4e5aec4eb956e699c11792e13539b93` | `aba926ab4ad41a81e4a7a67635d9b52c4d3878dd7bbe07ff3fa7d055f336adda` |
