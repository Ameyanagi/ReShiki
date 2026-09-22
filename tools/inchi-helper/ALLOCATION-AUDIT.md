# Native heap boundary

The source manifest pins InChI 1.07.3: 57 C translation units and 100 relevant
source/header files. The kernel source stays unchanged. Only those C units
receive `allocator_redirect.h` through the compiler's forced-include option.
`build.json` records the flags, source hashes and object-symbol audit.

## Coverage

`mode.h` offers `inchi_malloc`, `inchi_calloc`, `inchi_realloc` and
`inchi_free` macros, but these alone miss direct C calls. `bcf_s.c` calls
`calloc` and `free`; `mol_fmt3.c`, `ichiparm.c` and `ichiread.c` also contain
raw `free` calls. The redirect therefore covers both the macros and raw
`malloc/calloc/realloc/free` calls. System declarations are included first.

`USE_ALLOCA` is zero in the pinned source. The MSVC-specific `fast_alloc`
macro names `_alloca`, but has no call sites in these sources. The audit
found no direct kernel calls to mmap, VirtualAlloc or HeapAlloc. The Windows
branch of `ichiparm.c` calls `_strdup`, while Unix calls `inchi__strdup`.
Both `_strdup` and `strdup` redirect to an arena-backed copy, including its
terminating byte, so later redirected frees use the same allocator.
Null input returns null without allocating, matching a standalone MSVC `/MT`
probe and the kernel's Unix `inchi__strdup`; default option parsing uses it.
The builder additionally rejects bypass symbols in every compiled kernel
object using `nm` or MSVC `dumpbin`. It requires all four heap APIs and also
the string-copy API on Windows.
The arena's own object is deliberately outside that redirection and audit.

The arena allocates one block, rounded down to the maximum native alignment.
All payload, padding and block headers fit inside it. Allocation splits free
blocks; freeing coalesces adjacent blocks. Reallocation preserves existing
data, including when moving a block. Checked size calculations reject
arithmetic overflow. Calls remain single-threaded within one request process.

When a block cannot fit, the helper writes a fixed resource response with
`write` or `WriteFile`, then exits. It does not ask the kernel to continue
through an out-of-memory cleanup path. A failed initial host allocation is
a separate result. Native identifiers and diagnostics are compared with the
original adapter under the ordinary budget; bounded-memory failure outcomes
are intentionally separate from native chemistry statuses.

## What is outside the arena

- The request and response buffers each have an 8 MiB protocol bound.
- Native input arrays have at most 32,767 atoms and stereo records.
- The parent bounds diagnostic output to 64 KiB and individual strings to 2 MiB.
- Stack, executable mappings, C/C++ runtime internals and OS allocation
  bookkeeping are outside the kernel arena. Library operations such as
  sorting can allocate internally without calling the kernel's C symbols.

These are separate bounds, not a claim that total RSS equals the arena budget.
The arena is enforced on Linux, macOS and Windows by the same source. The
application continues to use a killable async process, with no Rust FFI.

## Additional OS containment

Linux `RLIMIT_AS` can restrict total virtual address space, including new
mapping and allocation growth. It does not measure RSS. This follows the
[Linux resource-limit API](https://man7.org/linux/man-pages/man2/getrlimit.2.html).

Windows Job Objects support a process committed-memory ceiling through
`JOB_OBJECT_LIMIT_PROCESS_MEMORY`. This differs from a working-set limit;
see Microsoft's [extended job limits](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information).

On the validation Mac (macOS 26.5.1, arm64), a separate `RLIMIT_AS` probe
allowed exactly 32 MiB of additional mappings before `ENOMEM`. The initial
virtual reservation was about 415 GiB, so a small absolute address-space
ceiling is inappropriate. The kernel's implementation is in Apple's
[resource limits](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_resource.c)
and [VM map checks](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/vm/vm_map.c).
A virtual-growth ceiling would still not be a total-RSS guarantee.

These OS mechanisms are evaluated separately; this checkpoint does not
install them or claim they provide identical limits across platforms.
