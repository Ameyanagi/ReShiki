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
These bounds cover the bridge's buffers, not total kernel allocations. The
official C allocator remains unchanged. Production integration still requires
resource containment on Linux, macOS and Windows, portable builds, packaging
and runtime dispatch before this helper is shipped to users.

All integer fields and IEEE-754 doubles use little endian:

| Frame              | Fields                                                                                                       |
| ------------------ | ------------------------------------------------------------------------------------------------------------ |
| Request header     | `RSHINCHI`, protocol u16=1, operation u8=1, coordinate-presence u8, payload length u32                       |
| Request payload    | atom count u16, stereo count u16, atom records, stereo records                                               |
| Atom               | xyz f64×3, zero-padded element char×6, isotope mass i16, charge i8, H i8×4, radical i8, bond count u8, bonds |
| Bond               | neighbor i16, type i8, direction i8                                                                          |
| Stereo             | central atom i16 (-1 if absent), neighbors i16×4, type i8, parity i8                                         |
| Response prefix    | `RSHINCHI`, protocol u16=1, version string (`1.07.3`)                                                        |
| Native result      | kind u16=0, native status i16, InChI/message/log/auxiliary strings                                           |
| Protocol rejection | kind u16=1, reason string                                                                                    |

A string is its u32 byte length followed by UTF-8 bytes. No options, shell
command, file path or second operation can be sent to the kernel. The helper
reports its version before running the request. Empty native identifiers and
all native return statuses remain distinct from process/protocol failures.
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
