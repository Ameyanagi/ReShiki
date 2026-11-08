# Frozen naming worker responses

Moving the naming engines out of ReShiki (see the [naming split plan](../plans/naming-split.md))
must not change any output. The [worker response snapshot](../structure-to-name-harness.md#worker-response-snapshot)
is that gate: it records every request and the exact response bytes of the naming
worker for the full, deduplicated bakeoff, openclatura and rules corpora.

The fixture holds 4,885 requests (3.2 MB) from 1,650 distinct SMILES:

| Request             | Count |
| ------------------- | ----: |
| Generate, IUPAC     | 1,650 |
| Generate, Boughton  |     1 |
| Verify (parse-back) | 1,617 |
| Parse (strict)      | 1,617 |

The 34 generation declines are frozen too. No current corpus result has a
radical-ion neutral-parent target; the updater adds that Verify request when one
appears.

Replay takes about 54 s in the normal test profile, so the complete gate is a
regular test. Two regenerations produced byte-identical files. A deliberate
whitespace change to 11 frozen responses failed the replay with 11 differences.

Responses include version metadata (`port_version`, `upstream_commit`,
`resource_fingerprint`, `protocol`). A step that only renames or re-versions a
crate regenerates the fixture, and its diff must be limited to those fields.
