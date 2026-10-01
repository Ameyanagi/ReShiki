# Rust helper allocation budget

`crates/process-heap/src/lib.rs` wraps Rust's system allocator. Every allocation receives an aligned
header recording its charge. Allocation size includes the header and alignment
padding. Budgeted reallocations use allocate/copy/free, so their temporary peak counts against the budget. The editor leaves the budget disabled and delegates growth to the system allocator, retaining in-place reallocations. Both modes use the same aligned allocation headers; the application never enables an operation budget in the editor process.

The budget starts after a byte-bounded request has been read and deserialized.
It covers validation, ReShiki toolkit callbacks, the cosmolkit-inchi kernel,
returned molecules and response serialization. Freeing request buffers created
before the budget starts cannot reduce usage, because their headers record a
zero charge. Live charged allocations are tracked with atomic counters.

Before asking the system allocator for memory, an allocation atomically checks
that its full charge fits. Exhaustion writes `RESHIKI_HEAP_LIMIT budget used
requested` to preinitialized stderr without allocating and exits with code 75.
The parent validates that marker and returns a typed resource error. No panic,
unwind, longjmp, partially generated identifier or recovery inside the allocator
is attempted. Other failures remain distinct process/protocol errors.

The default budget is 64 MiB and the maximum is 512 MiB. This is not an RSS
limit: executable images, thread stacks, allocator bookkeeping outside the
requested blocks, system libraries and initial request parsing are excluded.
System allocation failure can still terminate the process before the configured
budget is reached; it is reported as a process failure, not mislabelled as a
budget exhaustion.

The process boundary enforces the operation deadline and kills the child when
its future is dropped, including when the child stops reading stdin or exceeds
its output limit. Every request uses a new process, so allocator state is never
shared between chemical operations.
