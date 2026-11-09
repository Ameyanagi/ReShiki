# Offline retained-name catalog

`ReShiki retained aliases 1` is a finite local index: 85 chemical identities and
97 curated names. It supplements the existing systematic-name generator;
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
from the bundled OPSIN MIT resources, pinned to OPSIN `3.0-SNAPSHOT`
(commit `42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91`, the resource hashes and
fingerprint in [provenance.json](provenance.json)). The initial small-molecule
selection used OPSIN 2.9.0 (`b91b610af5ab07560fedb20730d7aef46bb2bca0`);
its original pin remains in the provenance, while every shipping alias is
validated against the current bundled parser and resources. Alias source links
point at the current pin. Evidence gives exact resource files and token
spellings. A stem can require a cited suffix or functional class to form the
complete alias: for example, `ethyl alcohol` combines `eth`, `yl` and `alcohol`.
These selections do not modify or exhaust the upstream vocabulary.

The biochemical reference graphs and spellings come from the frozen
[template catalog](../../../assets/template-catalog.json) (21 neutral free
amino acids, including L-selenocysteine, and five nucleobases) and
[Haworth sugars](../../../assets/haworth-sugars.json) (24 explicit anomers).
An asset name's trailing abbreviation, such as `(GlcNAc)`, is not part of the
alias.
`tautomer_smiles` takes precedence where the template specifies it, matching
the graph used by the application. Both asset files have pinned hashes in the
provenance. Amino-acid names retain their `L-` designation except achiral
glycine; template UI capitalization is normalized to conventional lowercase
stems (`L-alanine`, `glycine`). Sugar names retain the asset's Greek α/β spelling.

The bundled OPSIN plain names `adenine` and `guanine` strictly parse to the
**7H** graphs, so only those exact graphs receive the plain aliases. The usual
**9H** template graphs receive `9H-adenine` and `9H-guanine`. Their frozen 7H
alternatives also receive `7H-adenine` and `7H-guanine`. Every locanted alias
must reconstruct its own graph; no names are copied between tautomers.

Three further frozen asset graphs differ from the plain OPSIN defaults:
cytosine has the 3H tautomer and L-histidine has the opposite imidazole N-H
tautomer, while arginine places its
guanidine double bond on the chain nitrogen. The catalog therefore uses
`3H-cytosine`, `3-(1H-imidazol-5-yl)-L-alanine` and
`N(5)-(diaminomethylidene)-L-ornithine` for those exact reference graphs. The
plain `cytosine`, `L-histidine` and `L-arginine` spellings each have a separate,
independently specified record for the exact default graph from the strict
bundled OPSIN parser. The template records retain their original graphs and
aliases; matching never moves a name between these tautomers.

The one `(+)-lactic acid` alias uses the existing retained-name fact from
[ChEBI:422](https://www.ebi.ac.uk/chebi/CHEBI:422), record modified 18 April 2024,
accessed 11 October 2026. It identifies the explicit S graph
`C[C@H](O)C(=O)O`; ChEBI attributes this synonym to NIST. ChEBI website data is
[CC BY 4.0](https://www.ebi.ac.uk/chebi/about). The source attribution and full
legal code are in [licenses/name-aliases](../../../licenses/name-aliases/NOTICE).
OPSIN's [MIT license](../../../crates/nazuchem-n2s/LICENSE) remains separate from
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

## Biochemical coverage

The order here follows the appended catalog records. Configuration and tautomer
labels describe distinct graph identities, rather than display-only qualifiers.

| Record                         | Curated names                           | Explicit reference graph                             |
| ------------------------------ | --------------------------------------- | ---------------------------------------------------- |
| l-alanine                      | `L-alanine`                             | `C[C@@H](C(=O)O)N`                                   |
| l-arginine                     | `N(5)-(diaminomethylidene)-L-ornithine` | `C(C[C@@H](C(=O)O)N)CN=C(N)N`                        |
| l-asparagine                   | `L-asparagine`                          | `C([C@@H](C(=O)O)N)C(=O)N`                           |
| l-aspartic-acid                | `L-aspartic acid`                       | `C([C@@H](C(=O)O)N)C(=O)O`                           |
| l-cysteine                     | `L-cysteine`                            | `C([C@@H](C(=O)O)N)S`                                |
| l-glutamine                    | `L-glutamine`                           | `C(CC(=O)N)[C@@H](C(=O)O)N`                          |
| l-glutamic-acid                | `L-glutamic acid`                       | `C(CC(=O)O)[C@@H](C(=O)O)N`                          |
| glycine                        | `glycine`                               | `C(C(=O)O)N`                                         |
| l-histidine-3h                 | `3-(1H-imidazol-5-yl)-L-alanine`        | `C1=C(NC=N1)C[C@@H](C(=O)O)N`                        |
| l-isoleucine                   | `L-isoleucine`                          | `CC[C@H](C)[C@@H](C(=O)O)N`                          |
| l-leucine                      | `L-leucine`                             | `CC(C)C[C@@H](C(=O)O)N`                              |
| l-lysine                       | `L-lysine`                              | `C(CCN)C[C@@H](C(=O)O)N`                             |
| l-methionine                   | `L-methionine`                          | `CSCC[C@@H](C(=O)O)N`                                |
| l-phenylalanine                | `L-phenylalanine`                       | `C1=CC=C(C=C1)C[C@@H](C(=O)O)N`                      |
| l-proline                      | `L-proline`                             | `C1C[C@H](NC1)C(=O)O`                                |
| l-serine                       | `L-serine`                              | `C([C@@H](C(=O)O)N)O`                                |
| l-threonine                    | `L-threonine`                           | `C[C@H]([C@@H](C(=O)O)N)O`                           |
| l-tryptophan                   | `L-tryptophan`                          | `C1=CC=C2C(=C1)C(=CN2)C[C@@H](C(=O)O)N`              |
| l-tyrosine                     | `L-tyrosine`                            | `C1=CC(=CC=C1C[C@@H](C(=O)O)N)O`                     |
| l-valine                       | `L-valine`                              | `CC(C)[C@@H](C(=O)O)N`                               |
| adenine-9h                     | `9H-adenine`                            | `Nc1ncnc2[nH]cnc12`                                  |
| adenine-7h                     | `adenine`, `7H-adenine`                 | `C1=NC2=NC=NC(=C2N1)N`                               |
| guanine-9h                     | `9H-guanine`                            | `Nc1nc2[nH]cnc2c(=O)[nH]1`                           |
| guanine-7h                     | `guanine`, `7H-guanine`                 | `C1=NC2=C(N1)C(=O)NC(=N2)N`                          |
| cytosine-3h                    | `3H-cytosine`                           | `C1=C(NC(=O)N=C1)N`                                  |
| thymine                        | `thymine`                               | `CC1=CNC(=O)NC1=O`                                   |
| uracil                         | `uracil`                                | `C1=CNC(=O)NC1=O`                                    |
| alpha-d-glucopyranose          | `α-D-glucopyranose`                     | `C([C@@H]1[C@H]([C@@H]([C@H]([C@H](O1)O)O)O)O)O`     |
| beta-d-glucopyranose           | `β-D-glucopyranose`                     | `C([C@@H]1[C@H]([C@@H]([C@H]([C@@H](O1)O)O)O)O)O`    |
| alpha-d-galactopyranose        | `α-D-galactopyranose`                   | `C([C@@H]1[C@@H]([C@@H]([C@H]([C@H](O1)O)O)O)O)O`    |
| beta-d-galactopyranose         | `β-D-galactopyranose`                   | `C([C@@H]1[C@@H]([C@@H]([C@H]([C@@H](O1)O)O)O)O)O`   |
| alpha-d-mannopyranose          | `α-D-mannopyranose`                     | `C([C@@H]1[C@H]([C@@H]([C@@H]([C@H](O1)O)O)O)O)O`    |
| beta-d-mannopyranose           | `β-D-mannopyranose`                     | `C([C@@H]1[C@H]([C@@H]([C@@H]([C@@H](O1)O)O)O)O)O`   |
| alpha-d-ribofuranose           | `α-D-ribofuranose`                      | `C([C@@H]1[C@H]([C@H]([C@H](O1)O)O)O)O`              |
| beta-d-ribofuranose            | `β-D-ribofuranose`                      | `C([C@@H]1[C@H]([C@H]([C@@H](O1)O)O)O)O`             |
| alpha-d-fructofuranose         | `α-D-fructofuranose`                    | `C([C@@H]1[C@H]([C@@H]([C@@](O1)(CO)O)O)O)O`         |
| beta-d-fructofuranose          | `β-D-fructofuranose`                    | `C([C@@H]1[C@H]([C@@H]([C@](O1)(CO)O)O)O)O`          |
| l-histidine-opsin              | `L-histidine`                           | `N[C@@H](CC1=CNC=N1)C(=O)O`                          |
| l-arginine-opsin               | `L-arginine`                            | `N[C@@H](CCCNC(N)=N)C(=O)O`                          |
| cytosine-opsin                 | `cytosine`                              | `N1C(=O)N=C(N)C=C1`                                  |
| l-selenocysteine               | `L-selenocysteine`                      | `N[C@@H](C[SeH])C(=O)O`                              |
| 2-deoxy-alpha-d-ribofuranose   | `2-deoxy-α-D-ribofuranose`              | `C1[C@@H]([C@H](O[C@@H]1O)CO)O`                      |
| 2-deoxy-beta-d-ribofuranose    | `2-deoxy-β-D-ribofuranose`              | `C1[C@@H]([C@H](O[C@H]1O)CO)O`                       |
| alpha-d-xylopyranose           | `α-D-xylopyranose`                      | `C1[C@H]([C@@H]([C@H]([C@H](O1)O)O)O)O`              |
| beta-d-xylopyranose            | `β-D-xylopyranose`                      | `C1[C@H]([C@@H]([C@H]([C@@H](O1)O)O)O)O`             |
| alpha-l-fucopyranose           | `α-L-fucopyranose`                      | `C[C@H]1[C@H]([C@H]([C@@H]([C@@H](O1)O)O)O)O`        |
| beta-l-fucopyranose            | `β-L-fucopyranose`                      | `C[C@H]1[C@H]([C@H]([C@@H]([C@H](O1)O)O)O)O`         |
| alpha-l-rhamnopyranose         | `α-L-rhamnopyranose`                    | `C[C@H]1[C@@H]([C@H]([C@H]([C@@H](O1)O)O)O)O`        |
| beta-l-rhamnopyranose          | `β-L-rhamnopyranose`                    | `C[C@H]1[C@@H]([C@H]([C@H]([C@H](O1)O)O)O)O`         |
| n-acetyl-alpha-d-glucosamine   | `N-acetyl-α-D-glucosamine`              | `CC(=O)N[C@@H]1[C@H]([C@@H]([C@H](O[C@@H]1O)CO)O)O`  |
| n-acetyl-beta-d-glucosamine    | `N-acetyl-β-D-glucosamine`              | `CC(=O)N[C@@H]1[C@H]([C@@H]([C@H](O[C@H]1O)CO)O)O`   |
| n-acetyl-alpha-d-galactosamine | `N-acetyl-α-D-galactosamine`            | `CC(=O)N[C@@H]1[C@H]([C@H]([C@H](O[C@@H]1O)CO)O)O`   |
| n-acetyl-beta-d-galactosamine  | `N-acetyl-β-D-galactosamine`            | `CC(=O)N[C@@H]1[C@H]([C@H]([C@H](O[C@H]1O)CO)O)O`    |
| alpha-d-glucopyranuronic-acid  | `α-D-glucopyranuronic acid`             | `[C@@H]1([C@@H]([C@H](O[C@@H]([C@@H]1O)O)C(=O)O)O)O` |
| beta-d-glucopyranuronic-acid   | `β-D-glucopyranuronic acid`             | `[C@@H]1([C@@H]([C@H](O[C@H]([C@@H]1O)O)C(=O)O)O)O`  |

## Validation

`catalog_identity_is_order_invariant_without_relaxing_stereo_charge_or_isotopes`
checks order/map invariance, separate S/R/unspecified and E/Z entries, and
missing isotope/charge/uncatalogued variants.
`every_curated_alias_has_pinned_evidence_and_decodes_to_its_independent_graph`
checks every exact source token and strict native parser reconstruction of
every complete OPSIN alias, with success status and no warnings. The ChEBI
optical alias uses its explicit S normalization; the application's retained-name
tests cover that complete import path.

`catalog_provenance_pins_the_shipping_data_and_all_cited_resources` checks
catalog counts, the data checksum, the active OPSIN pin and fingerprint, every
cited resource hash, and the two reference-asset hashes.
`biochemical_aliases_match_the_exact_template_and_haworth_asset_graphs` requires
coverage of every selected asset graph.
`biochemical_aliases_keep_configuration_and_purine_tautomers_distinct` checks
that L-alanine aliases do not reach D, unspecified or zwitterionic alanine,
that 9H/7H adenine and guanine remain separate, and that α/β-D-glucopyranose
and unspecified anomeric configuration remain separate.
`biochemical_plain_opsin_defaults_keep_template_tautomers_distinct` checks
strict parser reconstruction and exact alias coverage of the three plain-name
default graphs, keeps their frozen template tautomers and aliases separate,
and rejects opposite or unspecified amino-acid configuration and zwitterionic
protonation.

Initialization is bounded to 64 KiB, 128 identities and five aliases per
identity. The shipping data is smaller. All graphs pass the existing 512-atom
naming identity preflight. The catalog has no mutable external resource or
network fallback. Adding records requires an explicit graph, provenance/license
review and these reconstruction tests.
