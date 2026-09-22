# Development InChI helper

This helper statically links the official InChI 1.07.3 C kernel. It does not
port that kernel to Rust. The Rust application uses an async child process and
does not load native code, RDKit or Python. Runtime dispatch and release
packaging are separate work.

Build from the audited, extracted `INCHI-1-SRC` directory:

```sh
python scripts/build_inchi_helper.py --source /path/to/INCHI-1-SRC
RESHIKI_REQUIRE_INCHI_HELPER=1 cargo test --test inchi_generator
```

The builder verifies all 100 relevant source/header hashes and compiles the
same 57 C files as pinned RDKit. `source-manifest.json` identifies the official
archive; `artifacts/inchi-helper/build.json` records actual source, compiler,
flags and executable hashes. Compilation is limited to four jobs. The output
depends only on system C/C++ libraries. The stub executable is test-only.
On Windows, run inside an initialized Visual Studio tools environment. The
builder defaults to `cl` with C11/C++20, `/MT` and MSVC arguments; `CC`/`CXX`
can select `clang-cl`. `--compiler-style msvc` selects that syntax explicitly.
Object names are checked for collisions before compilation.

Each process accepts one request. Both sides bound frames to 8 MiB, strings
to 2 MiB, atom/stereo counts to 32,767 and stored atom neighbors to 20. The
kernel applies its own chemical and standard-generation atom limits. The
parent separately limits stderr to 64 KiB and runtime to at most 120 seconds;
timeout, cancellation and rejected/oversized output terminate the child.
The native kernel's direct C heap uses a fixed arena: 64 MiB by default,
including payload, alignment and allocator metadata. A request may set a
budget from 1 byte to 512 MiB. Exhaustion terminates that process with a typed
`KernelHeap` resource result; the error path does not allocate. A host that
cannot allocate the arena returns `ResourceUnavailable` separately.

This is not a process RSS ceiling. Stack, system-library internals and bounded
bridge buffers remain outside the arena. See [the allocation audit](./ALLOCATION-AUDIT.md)
for coverage and platform limits. Portable release builds, packaging, runtime
dispatch and any additional OS containment remain separate integration work.

All integer fields and IEEE-754 doubles use little endian:

| Frame              | Fields                                                                                                       |
| ------------------ | ------------------------------------------------------------------------------------------------------------ |
| Request header     | `RSHINCHI`, protocol u16=2, operation u8=1, coordinate-presence u8, payload length u32                       |
| Request payload    | kernel heap budget u32, atom count u16, stereo count u16, atom records, stereo records                       |
| Atom               | xyz f64×3, zero-padded element char×6, isotope mass i16, charge i8, H i8×4, radical i8, bond count u8, bonds |
| Bond               | neighbor i16, type i8, direction i8                                                                          |
| Stereo             | central atom i16 (-1 if absent), neighbors i16×4, type i8, parity i8                                         |
| Response prefix    | `RSHINCHI`, protocol u16=2, version string (`1.07.3`)                                                        |
| Native result      | kind u16=0, native status i16, InChI/message/log/auxiliary strings                                           |
| Resource failure   | kind u16=2, scope u16=1, reason u16, budget/used/requested u64×3                                             |
| Protocol rejection | kind u16=1, reason string                                                                                    |

A string is its u32 byte length followed by UTF-8 bytes. No options, shell
command, file path or second operation can be sent to the kernel. The helper
reports its version before running the request. Empty native identifiers and
all native return statuses remain distinct from process/protocol failures.
Resource reasons distinguish budget exhaustion (1), host allocation failure
(2), and an allocator invariant failure (3). `used` counts live blocks and
their metadata; the fixed arena also contains free-block metadata. The
entire arena remains within `budget` regardless of that live counter.
The Rust input adapter's typed `NativeEmpty::TooManyNeighbors` outcome is
separate: native preparation returns an empty identifier before calling the
kernel and leaves its return-code field uninitialized. Rust retains the empty
outcome without inventing a status or treating it as a transport failure.

The end-to-end test compares Rust preparation plus this helper against the
original installed RDKit wrapper. Only the compiler/platform build-stamp line
in empty-input help is excluded from log comparison; chemical messages,
status, identifier and auxiliary information must match exactly. Separate
stub tests exercise timeout, cancellation, blocked stdin, invalid versions,
truncated/oversized output, malformed status and nonstandard identifiers.

The kernel is MIT licensed; retain `licenses/inchi/LICENSE` and `KERNEL-NOTICES`
when distributing it. Its SHA-256 source permission applies when distributed
as part of InChI. This helper includes the complete generation kernel, not a
standalone reuse of its hashing implementation.

Independent allocator tests cover alignment, calloc zeroing, realloc data
preservation, zero sizes, free/coalescing, fragmentation, overflow, repeated
requests and 20,000 mixed operations. `python -m unittest
tests.test_inchi_arena tests.test_inchi_helper_protocol` runs them alongside
the byte-level boundary tests after building the helper.
