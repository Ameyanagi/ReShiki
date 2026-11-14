# Naming IR migration gates

This record keeps the one-off checks that moved the naming worker to
`nazuchem_ir::Molecule` responses (protocol 7) on 2026-11-09. Their harness,
the `RESHIKI_OLD_S2N_SNAPSHOT` and `RESHIKI_NAZUCHEM_CHECKOUT` variables and the
`worker_snapshot::migration` tests no longer exist on `main`; they run only in a
worktree at commit `4ab8fd7b`. For the current replay, see the
[worker response snapshot](../structure-to-name-harness.md#worker-response-snapshot).

The IR worker uses protocol 7. The pre-migration fixture used protocol 6 and
contained 4,933 requests in 3,243,897 bytes. Parse and Verify
now return `nazuchem_ir::Molecule` instead of semantic CXSMILES, locant arrays
and axis side channels. Generate retains its SMILES input and existing result
semantics.

The one-off migration gates ran at commit `4ab8fd7b` on branch
`feat/naming-ir-protocol`. Their harness, `worker_snapshot_migration.rs`, was
removed after the migration. To rerun these historical gates, preserve the fixture
from main before that commit and create an isolated worktree at `4ab8fd7b`:

```sh
# 4ab8fd7b's parent is main before the IR migration.
git show 4ab8fd7b^:tests/fixtures/s2n/worker_snapshot.jsonl > /tmp/reshiki-10b-old-worker_snapshot.jsonl
git worktree add --detach /tmp/reshiki-ir-migration-4ab8fd7b 4ab8fd7b
cd /tmp/reshiki-ir-migration-4ab8fd7b
RESHIKI_OLD_S2N_SNAPSHOT=/tmp/reshiki-10b-old-worker_snapshot.jsonl cargo test --locked --lib -p reshiki naming::s2n_harness::worker_snapshot::migration::s2n_worker_ir_equivalence -- --ignored --exact --nocapture
```

The ignored migration test at that commit replays every preserved request through
the protocol-7 worker. It normalizes the protocol field
and requires Generate response bytes to match. For Parse and Verify, it checks
status, message, warnings, provenance, options and input name, then compares
canonical chemical identity, first parent locants, semantic CX layers and explicit
biaryl axes. The pinned IR keeps the old writer's atom identities as a prefix;
native hydrogen removal projects the prepared graph back to those identities.
The gate validates this map against per-atom chemical facts and the complete edge
set before comparing the literal parent-only first-locant vectors. Cases whose
canonical identity is unsupported on both paths are counted explicitly; their
mapped semantic groups and underlying specified stereochemistry must still match.
Set `RESHIKI_OLD_S2N_SNAPSHOT` to the fixture from main before `4ab8fd7b`, using
another path if needed. This semantic comparison is specific to the migration;
ordinary replay still compares the current wire response byte for byte.

The full migration-equivalence run on 2026-11-09 passed with **zero differences
and no skipped rows** across all 4,933 requests:

| Check                                           | Result                                                                                      |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Generate                                        | 1,654 responses byte-identical after changing only protocol 6 to 7                          |
| Parse/Verify envelope                           | 3,279 identical status, message, warnings, provenance, options and input-name results       |
| Parse/Verify without a structure                | 83 identical absent-structure results                                                       |
| Canonical chemical identity                     | 3,195 equal identities                                                                      |
| Canonical rejection with relative stereo groups | 1 matching rejection, with equal mapped membership and equal group-cleared specified stereo |
| Biaryl axes                                     | 2 equal mapped atom/reference/winding annotations                                           |

The companion size gate measures the snapshot requests and Parse/Verify requests
for distinct names from the pinned OPSIN oracle fixtures. It reports the largest
request and response for each operation against the unchanged 32,768-byte request
and 65,536-byte response limits, including explicit worker-error counts:

Run this gate from the same `4ab8fd7b` worktree, with a nazuchem checkout at its
pinned revision, `6fcd7c4f9ef7b892ae97aa042b5b4e13ee1aebe3`:

```sh
RESHIKI_OLD_S2N_SNAPSHOT=/tmp/reshiki-10b-old-worker_snapshot.jsonl RESHIKI_NAZUCHEM_CHECKOUT=/path/to/pinned/nazuchem cargo test --locked --lib -p reshiki naming::s2n_harness::worker_snapshot::migration::s2n_worker_ir_wire_sizes -- --ignored --exact --nocapture
```

The wire-size gate also passed on 2026-11-09. It measured the 4,933 snapshot
requests and 3,576 Parse/Verify requests for 1,788 distinct OPSIN oracle names:

| Measurement            | Snapshot maximum | Oracle maximum | Limit        |
| ---------------------- | ---------------- | -------------- | ------------ |
| Request, any operation | 349 bytes        | 349 bytes      | 32,768 bytes |
| Parse response         | 8,059 bytes      | 18,358 bytes   | 65,536 bytes |
| Verify response        | 8,072 bytes      | 18,371 bytes   | 65,536 bytes |

`undecadictane` produced both oracle response maxima. The largest response used
28.03% of the existing cap; the largest request used 1.07%. Two explicit request
rejections were the Parse and Verify requests for one oracle name containing an
embedded tab. No request or response was rejected by a wire cap, so both limits
remain unchanged.

The current protocol-7 snapshot was regenerated twice on 2026-11-09. Both runs
produced **4,933 rows and 8,020,627 bytes**, byte for byte identical, with SHA-256
`2188b9309fb252de6a2b29d777ace8c38413c0f2e8220e01b0ffb7a81cd72262`.
The ordinary byte-for-byte snapshot replay also passed.
