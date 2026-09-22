The two depiction geometry fixtures contain identical input bits and independent
calls to RDKit 2026.03.6, commit
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. Their first JSON line records the
platform, seven source hashes, native library hashes and helper executable hash.
Coordinates are hexadecimal IEEE-754 binary64 values. No Rust code, coordinate
alignment, zero cleanup or output rounding participates in their generation.

Each fixture has 5,310 cases: 795 ordered rings, 1,018 bisectors, 1,032 reflections,
1,372 canonical orientations and 1,093 boxes. Cases cover duplicate and permuted
ring IDs, empty/single fragments, signed zeros, repeated coordinates, degenerate
and underflowed axes, the exact eigenvector threshold and 279 nearby oblique
cases, varied bond lengths, random ordered fragments, and 160 fragment operations
on native layouts of 20 molecules. Of the 5,310 cases, 24 one-atom rings produce
native nonfinite output and return Rust `Error::Numeric`. Other successful cases
produce 71,754 scalar values. Inputs are repeated and checked for mutation.

The library preserves explicit, unfused source expression order. Linux x86_64
Rust 1.95 matches every Linux native output bit and every drawing-space f32 bit.
The test requires that exact comparison on Linux x86_64. Elementary arithmetic
and canonicalization must match the source-order fixture on other platforms too;
their platform libm results for ring embedding/reflection are reported as a
numerical audit. Set `RESHIKI_DEPICT_REQUIRE_EXACT=1` with a live oracle to require
bit equality on another host. The macOS fixture is a separate audit, not an exact
parity assertion. No epsilon converts these differences into a passing parity
claim. Both fixtures are retained so a later layout contract can assess them.

Linux Rust versus the pinned macOS ARM64 native fixture:

| Operation             | f64 differing / total | Maximum absolute error | Maximum relative error | f32 differing | Maximum drawing-space f32 error |
| --------------------- | --------------------: | ---------------------: | ---------------------: | ------------: | ------------------------------: |
| Bisector              |             0 / 2,036 |                      0 |                      0 |             0 |                               0 |
| Box                   |             0 / 4,372 |                      0 |                      0 |             0 |                               0 |
| Canonical orientation |       13,694 / 28,090 |  1.6443664208054543e-9 |     1.6357788920893093 |           352 |            1.300270469073439e-8 |
| Reflection            |           808 / 2,064 | 3.4924596548080444e-10 |                      1 |             3 |          2.5055675036914776e-15 |
| Ring                  |        1,850 / 35,192 |      5.340576171875e-5 |  1.722483919695817e-15 |             0 |                               0 |

Relative error is `abs(a-b)/max(abs(a),abs(b))`, with zero for two zeros. It can
be large at nominal zero; the audit also prints the scaled error using a minimum
denominator of one. The largest absolute ring error occurs with the deliberately
large `1e10` bond length. Canonical cancellation includes coordinates above `1e8`.
The macOS library disassembly contains `fmadd`/`fmla` for covariance accumulation,
the discriminant and vector lengths; its transform also contracts arithmetic.
Ring differences arise from the platform `sin`/`cos` implementation. No threshold
branch changed in this corpus. This does not prove that future near-threshold
fragments will take identical branches across native compiler builds.

The independent drawing projection uses `x*28`, `-y*28`, then converts to f32.
For native molecular fragment inputs, canonicalization has 119 differing f64
scalars and 10 differing f32 scalars, with maximum projected error
`7.213588806284166e-16`. There is no claim of exact cross-platform layout parity,
and this checkpoint is not connected to application layout dispatch. Numerical
overflow rejection is a typed geometry error, separate from editable-document
validation. Molecule chemistry and selection scope are outside this module.

Regenerate the macOS fixture from an isolated checkout with pinned Python RDKit,
Apple clang and Boost headers:

```sh
CC=/usr/bin/clang CXX=/usr/bin/clang++ .venv/bin/python \
  tests/build_depict_geometry_oracle.py --rdkit-source /path/to/pinned/rdkit
```

On Linux, use the pinned wheel's Python, a C++20 compiler and Boost headers. Replay
the existing input bits to keep cross-platform comparisons independent of the
platform that generated initial molecular layouts:

```sh
.venv/bin/python tests/build_depict_geometry_oracle.py \
  --rdkit-source /path/to/pinned/rdkit --boost-include /usr/include --replay \
  --output tests/fixtures/depict-geometry-linux-native.json.gz
```

On Windows, compile `tests/depict_geometry_reference.cpp` with C++20 and a matching
RDKit SDK from the pinned source. Include its `Code` tree and generated headers,
and link the Depictor, GraphMol, RDGeometryLib and RDGeneral import libraries.
The executable, DLLs, C++ runtime and Python wheel must share an architecture.
Place dependent DLLs on `PATH`, then use the reference script with `--oracle`,
`--rdkit-source`, `--replay`, `--write-fixture` and `--output`. The wheel alone does
not supply a complete Windows development SDK. No Windows C++ fixture has been
generated for this checkpoint.

For live replay during Rust tests, set `RESHIKI_DEPICT_ORACLE` to the executable
and `RESHIKI_RDKIT_SOURCE` to the source tree. The replay checks the seven pinned
source hashes and RDKit version. On macOS set `DYLD_LIBRARY_PATH` to the wheel's
`.dylibs`; on Linux set `LD_LIBRARY_PATH` to `rdkit.libs`. Then run:

```sh
RESHIKI_DEPICT_REQUIRE_EXACT=1 CARGO_BUILD_JOBS=4 \
  cargo +1.95.0 test --locked --test depict_geometry -- --nocapture
```

The default fixture reader uses only Python's standard library. Compressed and
uncompressed SHA-256 respectively:

- macOS: `506a455273e46c8d9190ded0dc93f1504bac3b74e89987ac3305f7823aeb9541`,
  `756498af0bf7f42ec968d6e5aa6ca8ed08979c57248848e57f22d46dfd5ae902`.
- Linux: `9a75f176ee4a23f401a41eae557de08f6027549eca728d566699b5d331307590`,
  `b179ff4f67a95ba61f6a9cebaf224c04cfbdddc77b3275cfc264c3cf2579ebd8`.

Adapted geometry source is attributed in `licenses/rdkit/NOTICE` under the
accompanying BSD-3-Clause license. The helper calls the five original C++ APIs;
Python `Compute2DCoords` is used only to supply additional fragment inputs.
