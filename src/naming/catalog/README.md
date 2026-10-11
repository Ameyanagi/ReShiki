# Offline retained-name catalog

`ReShiki retained aliases 1` is a finite local index: 30 chemical identities and
40 curated names. It supplements the existing systematic-name generator;
it is not a general synonym database or a preferred-name authority.

The application matches canonical isomeric graph identity, including specified
stereochemistry, isotopes and charge. Atom order and reaction map numbers do
not change that identity. A stereochemically unspecified graph is a distinct
entry from either absolute enantiomer; unspecified lactic acid is not labelled
as racemic. No tautomer, salt, protonation or optical-sign relaxation is used.
No entry means an empty alias list, never a guessed name. An unsupported or
invalid molecular identity remains an error.

## Sources and adaptation

Each alias in [data.json](data.json) cites a source. OPSIN aliases are selected
from the already bundled OPSIN 2.9.0 MIT resources, pinned to commit
`b91b610af5ab07560fedb20730d7aef46bb2bca0`. The evidence gives exact resource
files and token spellings. A stem can require the cited suffix or functional
class to form the complete alias: for example, `ethyl alcohol` combines
`eth`, `yl` and `alcohol`. These selections do not modify or exhaust the
upstream vocabulary. Resource hashes and the catalog checksum are in
[provenance.json](provenance.json).

The one `(+)-lactic acid` alias uses the existing retained-name fact from
[ChEBI:422](https://www.ebi.ac.uk/chebi/CHEBI:422), record modified 18 April 2024,
accessed 11 October 2026. It identifies the explicit S graph
`C[C@H](O)C(=O)O`; ChEBI attributes this synonym to NIST. ChEBI website data is
[CC BY 4.0](https://www.ebi.ac.uk/chebi/about). The source attribution and full
legal code are in [licenses/name-aliases](../../../licenses/name-aliases/NOTICE).
OPSIN's [MIT license](../../../crates/opsin/LICENSE) remains separate from
ReShiki's original MIT OR Apache-2.0 integration code. Binary packaging
includes these notices through the existing license collector.

## Coverage

Order below is the stable catalog display order. Every graph is independently
specified; names are not reverse-generated to manufacture the expected graph.

| Record             | Curated names                            | Explicit reference graph |
| ------------------ | ---------------------------------------- | ------------------------ |
| methanol           | `methanol`, `methyl alcohol`, `carbinol` | `CO`                     |
| ethanol            | `ethanol`, `ethyl alcohol`               | `CCO`                    |
| propan-1-ol        | `propyl alcohol`                         | `CCCO`                   |
| propan-2-ol        | `isopropyl alcohol`                      | `CC(O)C`                 |
| acetone            | `acetone`                                | `CC(=O)C`                |
| formic-acid        | `formic acid`                            | `C(=O)O`                 |
| acetic-acid        | `acetic acid`                            | `CC(=O)O`                |
| propionic-acid     | `propionic acid`                         | `CCC(=O)O`               |
| butyric-acid       | `butyric acid`                           | `CCCC(=O)O`              |
| isobutyric-acid    | `isobutyric acid`                        | `CC(C)C(=O)O`            |
| valeric-acid       | `valeric acid`                           | `CCCCC(=O)O`             |
| isovaleric-acid    | `isovaleric acid`                        | `CC(C)CC(=O)O`           |
| lactic-unspecified | `lactic acid`                            | `CC(O)C(=O)O`            |
| lactic-s           | `L-lactic acid`, `(+)-lactic acid`       | `C[C@H](O)C(=O)O`        |
| lactic-r           | `D-lactic acid`                          | `C[C@@H](O)C(=O)O`       |
| crotonic-acid-e    | `crotonic acid`                          | `C/C=C/C(=O)O`           |
| isocrotonic-acid-z | `isocrotonic acid`                       | `C/C=C\C(=O)O`           |
| benzene            | `benzene`, `benzol`                      | `c1ccccc1`               |
| toluene            | `toluene`, `toluol`                      | `Cc1ccccc1`              |
| phenol             | `phenol`                                 | `Oc1ccccc1`              |
| aniline            | `aniline`                                | `Nc1ccccc1`              |
| catechol           | `catechol`, `pyrocatechol`               | `Oc1ccccc1O`             |
| hydroquinone       | `hydroquinone`                           | `Oc1ccc(O)cc1`           |
| resorcinol         | `resorcinol`, `resorcin`                 | `Oc1cccc(O)c1`           |
| styrene            | `styrene`                                | `C=Cc1ccccc1`            |
| glycerol           | `glycerol`, `glycerin`                   | `OCC(O)CO`               |
| acrolein           | `acrolein`                               | `C=CC=O`                 |
| chloroform         | `chloroform`                             | `ClC(Cl)Cl`              |
| glyoxal            | `glyoxal`                                | `O=CC=O`                 |
| ethanolamine       | `ethanolamine`, `monoethanolamine`       | `NCCO`                   |

## Validation

`catalog_identity_is_order_invariant_without_relaxing_stereo_charge_or_isotopes`
checks order/map invariance, separate S/R/unspecified and E/Z entries, and
missing isotope/charge/uncatalogued variants.
`every_curated_alias_has_pinned_evidence_and_decodes_to_its_independent_graph`
checks each source token and strict native parser reconstruction of every
complete OPSIN alias. The ChEBI optical alias uses its explicit S normalization;
the application's retained-name tests cover that complete import path.

Initialization is bounded to 64 KiB, 40 identities and five aliases per identity.
The shipping data is smaller. All graphs pass the existing 512-atom naming
identity preflight. The catalog has no mutable external resource or network
fallback. Adding records requires explicit graph, provenance/license review
and these reconstruction tests.

Both catalog regressions passed in the 26-test v2 naming-library suite. Catalog,
parser and identity source remained unchanged for the v3 Details-control
revision; its 28 app tests and five explicit renderer tests passed separately.
Qualified macOS native checks and independent saved-file audits verified the
ethanol and S-lactic graphs and linked alias captions. Final v3 also verified
R/unspecified/no-alias Details results without inserting those graphs. This
follow-up remains under review and unreleased. See the
[review and compact identity audit](../../../docs/changes/offline-name-synonyms.md)
for the exact producers, capture conditions, history attribution and limits.
