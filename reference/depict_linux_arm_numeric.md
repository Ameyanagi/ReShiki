# GNU/Linux ARM64 depiction arithmetic

Pinned source: RDKit 2026.03.6, commit
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. The reference is the original public
`rdDepictor.Compute2DCoords` and the direct native stage observers documented in
the individual `depict_*.md` files. No expected coordinates come from Rust, and
coordinates are never realigned for comparison.

The directly adapted source identities are:

| Source under `Code/`                 | SHA256                                                             |
| ------------------------------------ | ------------------------------------------------------------------ |
| `GraphMol/Depictor/EmbeddedFrag.cpp` | `a3c55426a09deb23443e53cb2b59b4056e9d312b8fdb1c749da5033bc5a1eb33` |
| `Geometry/point.h`                   | `aa985364528748f3c94b0e0f969fe63cbeb2b830e93df26c5b22f8cee8bfb393` |
| `Geometry/Transform2D.cpp`           | `c8cf18d72544c276836d74a17312b7d425ed2cbeed67409ca09fe9b5c80d1a4f` |

## Native build identity

The CPython 3.12 wheel is
`rdkit-2026.3.6-cp312-cp312-manylinux_2_28_aarch64.whl`, SHA256
`16d446583b7bac24b018a424bb24fd515c625f7bad4171d6aa46993a3e2f9aa0`.
Its ELF `.comment` identifies GCC 14.2.1 20250110 (Red Hat 14.2.1-11).

| Library                               | SHA256                                                             |
| ------------------------------------- | ------------------------------------------------------------------ |
| `libRDKitDepictor-b56ca7ed.so.1`      | `bf819ac0a33788bff9c0f5210f9b2452da037da4da75fe5bff9149afd31485e4` |
| `libRDKitRDGeometryLib-d5d214dc.so.1` | `d6f82698bc5781829e1e147d2e23b9c8c351d7056ff1b0ce5d53db34d54b22dd` |
| `libRDKitGraphMol-c9158b4b.so.1`      | `568eb5e6f02cb900a6b0fff45b16f4609dc919713d1ab8df744de3cf68271ba8` |
| `libRDKitRDGeneral-d34a92d4.so.1`     | `f082cead901bac9ec66484a3dc65d9250ee102e9d1574f9db2cb3bac37a51e27` |

The isolated validation container uses Ubuntu 24.04 ARM64, glibc
2.39-0ubuntu8.9, Python 3.12.3, and Rust 1.95.0
(`59807616e1fa2540724bfbac14d7976d7e4a3860`, LLVM 22.1.2). Direct observers use
GCC 13.3.0 and the wheel's exact Boost 1.85 headers. Original native library calls
supply the expected stage results; source adapters retain their existing source
hash and original-call checks. Ubuntu's default linker needs a process-local
`-Wl,--no-as-needed` option to retain the explicit Python library used by
transitive Boost DSOs.

## Instruction evidence

Addresses below belong to those exact ELF libraries. `Geometry` means
`RDGeometryLib`; all other entries belong to `Depictor`.

| Native site                           | Instructions                                                      | Rust policy                                                                                           |
| ------------------------------------- | ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Geometry `Point2D::lengthSq`          | `128a4` fmul of y; `128a8` fmadd of x                             | Fuse x² with separately rounded y²                                                                    |
| Geometry `TransformPoint`             | `14814`/`14818` fmul; `1481c`/`14820` fmadd; `14824`/`14828` fadd | Fuse the first product, then add translation separately                                               |
| Geometry transform alignment          | `148ac`–`148c0` norms/dot; `148fc` fnmsub                         | Preserve the first-product FMA and cross-product subtraction                                          |
| Canonical covariance                  | `37f34`, `37f38`, `37f3c` fmadd                                   | Fuse each product into its ordered accumulator                                                        |
| Canonical discriminant                | `37f5c`/`37f60` fmul; `37f64` fmadd                               | Round `4*xy*xy`, then fuse the squared difference                                                     |
| Canonical eigenvector lengths         | `37fac` rounds x²; `37fb4`/`37fd0` fuse y²                        | Use a separate canonical-length helper; generic Point2D length has the opposite product order         |
| Fused-ring mirroring                  | `47ba8` fnmsub; `47bac`, `47bd4` fmadd; `47bd8` fnmsub            | Reuse the verified ordered product helpers                                                            |
| Attachment with an angle              | `3cd54` fmul; `3cd58` fmadd; `3cedc` fnmsub                       | Preserve ordinary normal length and cross-product order                                               |
| Attachment without an angle: position | `3d424` vector fmla                                               | Fuse each bond-length scale with the reference coordinate                                             |
| Attachment without an angle: normal   | `3d434` fmul; `3d438` fmadd                                       | Normalize using the direction's length before its components are swapped for the perpendicular normal |
| Collision distances/cross products    | `3fa2c`, `3feac` fmadd; `3fe9c`/`3fea0` fnmsub                    | Reuse the verified norm and subtraction helpers                                                       |

The attachment position contraction spans the source's consecutive `*=` and
`+=` statements. The two special length sites retain their source-specific
operation order. Changing every norm to fuse the same component would break
other native calls.

The paired trigonometric policy is a separate prerequisite documented in
[depict_linux_math.md](depict_linux_math.md). This wheel calls `sincos@plt` at
`Depictor:4a49c` for ring embedding and `Geometry:14910` for transform alignment.
The safe optimized wrapper remains the only trigonometric adaptation.

## Strict validation

The unchanged public corpus first reproduced the CI failure exactly: 1,786
coordinate differences among 2,854 cases. The source-site corrections yield
2,758 exact successes and 96 matching checked rejections, with 130,722 coordinate
scalars compared bit for bit and no differences.

All eight original native stage suites also pass on this ABI: geometry, rings,
attachment, seeds, templates, expansion, collision, and finalization. Together
these compare 2,382,229 finite f64 scalars exactly and independently check their
drawing-space f32 projections. Existing native nonfinite/invalid cases keep their
checked failure behavior. The debug replay passes all 26 stage tests and both
public pipeline tests. Release-profile geometry and the full public pipeline
independently repeat those exact results. No tolerance or mismatch-count
acceptance was added.

Use live observers on ARM64. The static Linux x64 files are cross-ABI audit data,
not ARM64 expected outputs. The existing hooks are `RESHIKI_DEPICT_ORACLE` for
geometry, `RESHIKI_DEPICT_{RINGS,ATTACHMENT,SEEDS,TEMPLATES,COLLISION,FINALIZE}_ORACLE`
for those stages, and `DEPICT_EXPANSION_ORACLE` for expansion. Set
`RESHIKI_RDKIT_SOURCE` and `DEPICT_EXPANSION_SOURCE` to the pinned source checkout.
The finalization executable is `artifacts/depict-finalize-oracle/oracle`; the other
executables are `artifacts/depict-<component>-oracle`.

The new branches apply only to GNU/Linux AArch64. Existing macOS and x64 arithmetic
and all layout ordering, bounds, and error checks are retained. Other ARM64 ABIs
require their own native evidence before selecting this GNU policy.
