# Frozen naming worker responses

Moving the naming engines out of ReShiki (see the [naming split plan](../plans/naming-split.md))
must not change any output. The [worker response snapshot](../structure-to-name-harness.md#worker-response-snapshot)
is that gate: it records every request and the exact response bytes of the naming
worker for the full, deduplicated bakeoff, openclatura and rules corpora.

The fixture holds 4,933 requests (3.2 MB) from 1,653 distinct SMILES:

| Request                                          | Count |
| ------------------------------------------------ | ----: |
| Generate, IUPAC                                  | 1,653 |
| Generate, Boughton                               |     1 |
| Verify (generated, alternative and parent names) | 1,658 |
| Parse (strict)                                   | 1,621 |

Generation declines are frozen too. The corpora never populate some optional
response fields, so a reordered field there would go unnoticed. Three SMILES and
one name from the unit tests fill them: a radical cation named through its
neutral parent, two unknown-configuration inputs and an atropisomer name. The
test fails if any of these shapes disappears from the fixture.

Generate inputs are the corpus SMILES as written. The app first canonicalizes
them and may add unknown-configuration marks; the two marked inputs above cover
that path, but not every corpus input in its marked form. Names typed by users
(trivial, CAS) are covered by the nazuchem-n2s crate's own oracle tests, not here.

Replay takes about 55–70 s in the normal test profile, so the complete gate is a
regular test. Two regenerations produced byte-identical files. A deliberate
whitespace change to 11 frozen responses failed the replay with 11 differences
(checked on the first version of the fixture).

Responses include version metadata (`port_version`, `upstream_commit`,
`resource_fingerprint`, `protocol`). A step that only renames or re-versions a
crate regenerates the fixture, and its diff must be limited to those fields.
