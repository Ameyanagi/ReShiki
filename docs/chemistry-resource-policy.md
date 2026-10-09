# Calculation resource policy

Geometry and aromaticity use the same machine observations and resolve an
immutable operational budget for each calculation. Processor count does not
determine the budget. A bounded lookup probe measures effective throughput in
the current executable; it is a proxy, not a prediction of molecular solve time.
The probe runs once per process, targets 10 ms, and has a fixed iteration bound.
Memory observations refresh at most once per second.

The [desktop validation record](adaptive-chemistry-limits-validation.md) includes
matched former-512-atom rejection/3D-preview captures, the large fused-ring
import failure/success comparison, original fixtures and independent saved-file
chemical checks. The candidate remains under review.

## Observations and fallback

The memory observation is available headroom, rather than installed RAM:

- Linux takes the minimum of available host memory, finite process address-space
  and data-segment headroom, and readable cgroup memory headroom at the leaf and
  every ancestor.
- macOS uses native Mach free plus inactive pages, capped at half physical RAM,
  and applies finite process memory limits when available.
- Windows uses native `GlobalMemoryStatusEx` physical, available current-process
  commit, and virtual-address headroom.

A missing observation uses a 1 GiB memory reference or five million lookup
operations per second. A readable exhausted-memory limit stays zero and denies
calculation reservations. Missing host information does not discard readable
process or container ceilings. If a finite process or container constraint is
known but its current usage cannot be read, headroom is conservatively zero.
The platform snapshot cannot guarantee that
another process will not consume memory after admission.

| Resolved resource | Formula and independent bounds | Unknown observations |
| --- | --- | --- |
| Geometry allocator budget | headroom / 4, clamped to 64–512 MiB | 256 MiB |
| Geometry deadline | 300 million / lookup rate seconds, clamped to 60–120 s | 60 s |
| Aromaticity work | lookup rate × 10, clamped to 5–200 million units | 50 million |
| Aromaticity deadline | 10 s | 10 s |
| Aggregate calculation reservations | minimum of headroom / 2 and 1,280 MiB | 512 MiB |

A faster throughput observation never reduces the historical 60-second geometry
deadline. The independent wall-clock bound remains effective for cases whose
cost is poorly represented by the lookup probe. Explicit development-client
limits must still satisfy the 120-second and 512-MiB ceilings.

## Reservations and cancellation

Geometry reserves its allocator budget plus 32 MiB of process overhead before
launching a disposable worker. Waiting consumes the request deadline and
dropping the asynchronous operation cancels waiting or the worker exchange.
The worker allocator enforces charged live Rust allocation bytes; this is not a
total-process memory cap, since stack and operating-system allocations are
separate. All reservations release on return or cancellation.
Reservation accounting is shared within one application process; separate
ReShiki instances do not share a reservation counter.

Aromaticity reserves a checked estimate based on atoms, bonds and ring slots,
plus 16 MiB of overhead. This admission estimate is not an in-process allocator
cap. Cancellation and deadlines are checked before and after perception and
periodically during charged work. Failure returns an error without a partially
perceived graph. These reservations cover geometry and aromaticity; they do not
account for every application or operating-system allocation.

## Structural and chemistry constraints

Geometry retains the historical 256-MiB / 512-atom admission anchor. Below
that anchor it permits one original atom per 512 KiB of allocator budget; above
it, pair-table growth requires one additional original atom per 2 MiB.
Original bonds are limited to four times that atom envelope;
all-atom coordinates are limited to eight times it. Independent structural
ceilings remain 640 original atoms, 4,096 original bonds and 4,096 coordinates,
including temporary hydrogens. Thus the 64/256/512-MiB budgets admit at most
128/512/640 original atoms. Eligibility does not guarantee parameter coverage,
embedding success, convergence or completion within the worker deadline.
The worker independently validates the requested envelope against its heap
budget and the structural ceilings.

The framed protocol accepts a request without the new optional `capacity`
field. Such a legacy request retains the 512-atom and 2,048-bond envelope,
intersected with its heap-derived envelope. Normal clients relaunch their own
executable; development clients must use a worker with the matching protocol
schema. This compatibility does not allow a new request to an old worker.

Aromaticity retains the default model's six-ring combination limit and its
two-ring limit for fused systems larger than 300 rings. These are chemistry
rules, independent of the operational work budget. Ring-slot and ring-neighbor
ceilings also remain in force.

## Connected fused-ring enumeration

Singleton rings and adjacent pairs are evaluated directly. Larger combinations
are streamed in the former exhaustive lexicographic order of fused DFS
positions. A sorted prefix may be disconnected: the path `0–2–1` must still
emit `[0,1,2]`. A bounded 0–1 breadth-first search charges selected vertices zero
and future vertices one, excluding already skipped positions. It prunes only
when a selected vertex cannot be reached with the remaining slots or too few
future vertices can be reached. Every complete connected combination therefore
remains eligible. No full collection of combinations or exhaustive fallback is
stored.

The existing stop condition runs only between complete combination-size levels.
Dense systems can still require combinatorial work and fail the operational
budget. Successful runs preserve atom and bond markings and aromatic-ring
counts; work-limit success and failure boundaries intentionally improve with
the new enumeration.

## Validation and limits of the envelope

The issue's 37-repeat fused-ring input has 452 atoms and 150 rings. The previous
enumeration exceeded its 50-million-unit budget; the new enumeration completes
with 385,972 charged units. Exhaustive connected-subset checks cover every
labeled graph through five vertices and compare output order. The independent
aromaticity reference check also covers 31,019 cases, including 17,307 transformed
graphs. The six-ring and greater-than-300-ring rules remain unchanged.

Actual seeded geometry probes exercise Generate, Evaluate and pinned Relax at
513 and 640 original carbon atoms under MMFF94, MMFF94s and UFF, checking finite
coordinates, hydrogen mappings, energy agreement, finite analytic gradients and
unchanged pins. At 640 original carbons, UFF generation used about 421 million
charged live allocation bytes under the 512-MiB worker budget. Larger 768- and
1,024-carbon probes exhausted that budget safely, so they are not admitted.
The conservative 640-atom ceiling retains headroom; it is not unlimited scaling
with installed RAM. The new framed 511/512 controls retain the baseline's exact
seeded coordinates, energy and hydrogen mappings.

Development builds optimize `cosmolkit-core` at level 3, matching the numerical
test profile. A 513-carbon, 1,541-coordinate ordinary 500-iteration calculation
still exceeded a resolved 60-second deadline; an explicit, valid 120-second
developer request completed in approximately 69 seconds with `converged=false`.
The retained calculated carbon seed subsequently completed an ordinary request
in approximately 73 seconds under its resolved 75-second deadline. A separate
513-original-atom alkane with 171 carbons and 342 explicitly drawn hydrogens
completed the ordinary 500-iteration request in approximately 9 seconds under
its resolved 60-second deadline, returning 515 all-atom coordinates. Both
ordinary controls preserved the source drawing and reported `converged=false`.
Thus eligibility and a complete result are distinct from convergence. Timed-out
work returns no geometry to apply. These measurements establish bounded
capability on the tested machine, not a timing guarantee for every eligible
molecule or machine. Cross-platform resource probes require runtime validation
on their respective platforms.
