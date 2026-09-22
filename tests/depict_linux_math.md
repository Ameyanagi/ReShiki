# Linux paired trigonometry

The pinned RDKit wheels call `sincos` when both values are needed. Rust now uses
the existing safe `reshiki-depict-math` wrapper for Linux x64 and ARM64, as well
as macOS ARM64. The wrapper's dev/test optimization remains scoped to that tiny
crate; application optimization, coordinates and comparisons are unchanged.

## Ubuntu 22.04 x64

The original `00b8f1d` pipeline reproduced all 36 CI differences in an isolated
Ubuntu 22.04 container on the Arch Linux host. glibc 2.35 rounds some combined
`sincos` results differently from separate `sin` and `cos` calls. For example,
the phase `2π / 20 × 19`, with binary64 bits `4017e0485cda5e0a`, produces:

| Native call           | Sine bits          | Cosine bits        |
| --------------------- | ------------------ | ------------------ |
| Separate `sin`, `cos` | `bfd3c6ef372fe953` | `3fee6f0e134454ff` |
| Combined `sincos`     | `bfd3c6ef372fe954` | `3fee6f0e134454ff` |

Direct calls to the container's `libm.so.6` establish this difference; neither
the Rust solver nor stored molecule coordinates are used as the oracle.
Disassembly of the pinned wheel confirms `sincos@GLIBC_2.2.5` at
`RDDepict::embedRing + 0xdb` (`0x4662b`). RDGeometryLib also calls that symbol
at `0x10beb`, `0x116c0`, `0x14c65` and `0x14e26`. The optimized Rust wrapper's
object imports `sincos`, with no separate `sin` or `cos` import.

After the call correction, the unchanged public-API corpus passes exactly:
2,854 cases, 2,758 successes, 96 classified rejections and 130,722 successful
coordinate scalars. The same correction removes the larger `nci/2892` layout
divergence. Arch Linux also passes the full corpus and its geometry suite.

No coordinate override, tolerance, alignment, threshold adjustment or copied
libm implementation is involved. Existing macrocycle, high-degree atom and NCI
cases provide the regression coverage.

## Reference ABI boundaries

OS and architecture alone do not identify the numerical ABI. After the fix,
Ubuntu's geometry, ring, attachment and template suites differ from static
Arch captures while agreeing with the host's original public native pipeline.
Live validation replays these inputs through the original wheel on the tested
host, with exact comparisons. Routine Ubuntu 22.04 x64 tests can use the
[saved independent captures](depict_ubuntu_goldens.md). Collision, expansion,
finalization and seed captures also passed unchanged on this Ubuntu host.

Fresh direct-native observers pass geometry (5,310 cases), rings (1,338 cases),
attachment (970 cases) and templates (695,876 compared fragment scalars), with
zero unequal bits. They use the exact pinned Git source, Boost 1.85 headers and
the installed wheel. On Ubuntu the observer linker needs `--no-as-needed` to
retain libpython; this is test tooling only.

Linux ARM64 independently uses paired `sincos`: pinned Depictor ring site
`0x4a49c` and RDGeometryLib transform site `0x14910`. ARM64 contraction rules
are a separate audit; this change only selects the verified combined call.

## Reproduction identity

- Source: RDKit `0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`, version 2026.03.6.
- Compiler: Rust 1.95.0, ordinary test profile; four build jobs.
- Image: `ubuntu:22.04`, digest `b8b6ee6aa931ecd9d0d952abc34dc0e5f7c6a30c6bb71b079fe399fde0329c02`.
- libc: Ubuntu `2.35-0ubuntu3.14`.
- Depictor SHA-256: `278c40c5697437e65b4c9655b2530ccc68727081ea2d82364b4548bec333be55`.
- RDGeometryLib SHA-256: `b0cc4f7bac29abb006aed7d9db99837571dfe51065555760b6940a9ee41d434d`.
- libm SHA-256: `3dd5511ae94785c9f921429b0f2b2f7aabb461b6f0e6de6dfdbef15f24bdfee6`.

Native source remains covered by `licenses/rdkit`. Test-only observers use the
matching Boost 1.85 headers and the original wheel; no observer is shipped.
