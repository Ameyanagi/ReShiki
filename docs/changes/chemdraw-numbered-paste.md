# Read ChemDraw's numbered attachment points

ChemDraw 26 can include binary property `0x044b` in contracted groups such as SiMe3. ReShiki previously stopped with `Unsupported binary drawing property 0x044b` before it could read the molecule. The binary codec now preserves this property as `ExternalConnectionNum`, a positive signed byte on an ExternalConnectionPoint node, and the checked abbreviation importer consumes the explicit attachment definition.

This is a typed compatibility addition. Wrong node applicability, invalid byte length/range, duplicate number or NodeType properties, duplicate attachment numbers, incomplete numbering, and ambiguous ordering fail with diagnostics. Existing unknown-property rejection, bond-order checks, and the common internal anchor requirement remain. Numbered multiple attachments require an explicit ConnectionOrder together with the existing BondOrdering.

The [original controlled fixtures](../../tests/fixtures/numbered-attachments/README.md) were opened and saved through the real ChemDraw Prime 26.0.0.6599 desktop application on macOS. All three actual `.cdx` files reproduce the exact unsupported-property error in the unchanged Rust decoder at base `51fa0991da2507bb00b27c1b420e807468de6423`. The [producer receipt](../../tests/fixtures/numbered-attachments/producer-receipt.json) records exact file hashes, the read-only producer metadata identity, and the before diagnostics. No proprietary source, binary, or third-party sample is included.

The [archived primary CDX SDK table](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/TableOfProperties.htm) predates `0x044b`. Its modern name and applicability were established from installed producer metadata and the native node writer, then checked against real producer outputs. They are not claimed as entries in the archived specification.

ChemDraw keeps attachment numbers 2/1 and 3/1 while rewriting ConnectionOrder and BondOrdering together. Those lists pair object IDs; the numbers are not dense ordinal indices or a sort rule. The codec round trip retains both the sparse numbers and explicit lists. Supported common-anchor definitions become ordinary chemical bonds; their external-point placeholders are removed only after validation.

| Native controlled fixture                | Expected chemical result                                  | Numbered attachment data  |
| ---------------------------------------- | --------------------------------------------------------- | ------------------------- |
| Methoxytrimethylsilane                   | 6 atoms, 5 bonds; Si has 4 neighbors; O has 2             | 1                         |
| Dimethyldimethoxysilane                  | 7 atoms, 6 bonds; Si has 4 neighbors; both O atoms have 2 | 2, 1; explicit ID pairing |
| Same two-point group with sparse numbers | Same molecule and attachment graph                        | 3, 1; explicit ID pairing |

The unchanged base rejects all three native fixtures in both macOS and Windows codec reproductions. Head passes typed codec round trips, all 95 active IO tests, and the full engine graph and molecular-identity regression for native import, authored CDXML, and CDX export/reimport. The production CLI accepts all three and preserves CDXML, SVG, PNG, and analysis artifacts. Python codec regressions, lint, format, and type checks also pass, as do locked all-targets/all-features Clippy and check, and workspace format validation. Matched ReShiki desktop evidence is pending. The user authorized these controlled silicon fixtures for reproducing the issue. They establish native CDX input compatibility; Windows PowerPoint clipboard acceptance remains pending desktop access.

Reproduce with `cargo test -p reshiki-io exchange::tests` and `cargo test -p reshiki-io chemistry::cdxml::abbreviations::tests`, then `cargo test --test clipboard_exchange`. Open or import the exact native fixtures on base and head with the same window, style and zoom; compare the error against the accepted drawing and inspect the graph. Keep full `.rsk`, CDXML/CDX, molecular identity and exported figure receipts alongside the screenshots.

Caption: **Import ChemDraw's numbered silicon groups while preserving their attachment graph and rejecting ambiguous data.**
