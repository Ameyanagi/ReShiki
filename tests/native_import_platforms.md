# Original import outcomes that differ by platform

The complete import tests compare the original worker on the same host. Their
success totals differ because the pinned native CX coordinate reader accepts
different hexadecimal numbers on each platform. These are existing policies in
`src/chemistry/cx/number.rs`; no layout or import implementation was changed for
this audit.

| Corpus case                                        | Input detail                                               | macOS ARM64     | Linux x86_64    | Windows x64     |
| -------------------------------------------------- | ---------------------------------------------------------- | --------------- | --------------- | --------------- |
| `molecule/0/substance groups/SMARTS grammar/8106`  | RXN reactant DAT SMARTSQ `C \|(0x1)\|`                     | Query rejection | Accepted        | Query rejection |
| `molecule/1/substance groups/SMARTS grammar/8044`  | RXN product DAT SMARTSQ `C \|(0x1.fffffffffffff8p-1023)\|` | Query rejection | Accepted        | Query rejection |
| `smiles/cx/8094/C \|(0x1.1p-1075)\|`               | Hexadecimal subnormal coordinate                           | Parse rejection | Parse rejection | Accepted        |
| `smiles/cx/8191/C \|(0x1.123456789abcdefp-1074)\|` | Hexadecimal subnormal coordinate                           | Parse rejection | Parse rejection | Accepted        |

For the two RXN cases, the native Linux stream conversion rejects hexadecimal
CX input. `MolFileParser.cpp::processSMARTSQ` then ignores the failed SMARTS
parse, leaving the ordinary `AtomAtomicNum 6 = val` wrapper. The original
reaction importer permits that wrapper. macOS and Windows parse the SMARTS and
replace the query with `AtomType 6 = val`, which the original worker rejects
with `Query reaction atoms are not supported yet`.

The two standalone SMILES cases are rejected with `Could not parse this
structure` on macOS and Linux. Windows accepts rounded hexadecimal subnormals:
both input X coordinates become the smallest positive f64, hexadecimal
`0x0.0000000000001p-1022` (bits `1`). The original import subsequently replaces
these CX coordinates with its ordinary 2D layout.

This explains RXN totals of 2,220 complete / 1,506 failed on macOS versus
2,222 / 1,504 on Linux, and layout totals of 486 / 231 on macOS and Linux versus
488 / 229 on Windows. Raw native RXN calls additionally contain the same 78
responses that fail the app's final document validation on both hosts; those
are unrelated to the two native query differences.

`native_import_platform_reference.py` reproduces the four original calls from
`fixtures/native-import-platform-cases.json`, ten times each. It fails if a
repeated outcome or complete-response hash changes. Captured results in
`fixtures/native-import-platform-observations.json` record native query text,
coordinate bits, input hashes, response hashes, and host versions. All 120
focused calls were stable. Input hashes match across all three hosts. Separate
full-corpus processes agree with the focused outcomes and response hashes
(macOS/Linux RXN and all three layout captures).

The audit used RDKit `2026.03.6`, Boost `1_85`, and Python 3.12 on every host.
Native source revision is `0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`:
`CXSmilesOps.cpp:431-466` converts coordinates through
`boost::lexical_cast<double>`; `MolFileParser.cpp:2921-2980` applies or ignores
the DAT SMARTSQ query. The original `engine/reactions.py` accepts only the exact
atomic-number query wrapper. No Cargo execution or production edit was needed
for this audit.
