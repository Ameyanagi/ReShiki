# Synthetic ChemDraw import corpus

112 deterministic cases, seed `20261117`. These inputs were
constructed from published CDXML/CDX semantics and the independent Python
reference encoder. They are **not native ChemDraw-authored files**. ReShiki's
exporter is not used to construct them.

One generator-local tag correction follows native evidence: the published SDK
and Python reference identify arrows as `0x8027`, while native ChemDraw 26 uses
`0x8021` (also recorded in `crates/io/src/exchange/mod.rs` and retained native
captures). On 2026-11-17, a native spot-check of case 007 showed the blue arrow in
the CDXML but lost it in the reference-tag CDX. Generated CDX therefore uses
`0x8021`. The reference encoder still packs all properties and object framing;
no ReShiki drawing exporter is involved.

Regenerate: `python3 scripts/generate_chemdraw_import_corpus.py`
Verify checked-in bytes: `python3 scripts/generate_chemdraw_import_corpus.py --check`
Use `--seed`, `--extra-cases N`, and `--output` for fresh batches outside this folder.
The default appends eight fresh mixed drawings (105–112), checked in after the
original failures were fixed; `--extra-cases 0 --output DIR` reproduces the
retained base in another directory.
The seed controls geometry perturbations, rotations, molecular panel variation,
captions, icon-stamp transforms, and tiny image pixels. Feature families and
negative cases stay stable. The first 104 CDXML/CDX source bytes and paths stay
unchanged; fixed historical failures retain their original directory and a
`historical_failure` manifest note. Cases 001–064 retain their original IDs; cases 065
onward add true multi-page figures, multiple reaction schemes per document,
multistep scheme graphs, 10–60 editable vector stamps, repeated bitmap stamp
grids, mixed figure panels, and explicit fragment-scale controls.

`manifest.json` records each case's provenance, source inventory, expected visible
objects and counts, preservation checks, rejection reason and SHA-256 digests.
The supported partition is separate from valid but currently unsupported objects
and deliberately malformed inputs. Expectations describe intended behavior;
execution results belong in a separate report, not in fixture provenance.

Every encodable full drawing has `.cdxml`, `.cdx`, and `.drawing.cds` companions.
The CDS is full stationery: it contains the drawing objects plus packed
`LabelStyle` (`0x080a`) and `CaptionStyle` (`0x080b`) document defaults. The runner
imports this drawing through the CDX production pipeline and also exports actual
CDS via ReShiki's CDXML exporter and `exchange::to_cds`, then reimports it.
The translucent ordinary-graphic case has an explicit missing CDX/CDS limitation:
the independent encoder has no binary `alpha` property; it is never stripped.
Four style cases separately supply empty-artwork `.style.cdxml` and `.cds`
controls (`style_cds` in manifest schema 2). Compact stationery also retains
padded 28-byte-header and rootless legacy style variants. The probe's default
`.cds` mode loads styles; `--drawing-cds` selects the full-drawing mode.

Object IDs are unique within every input, including the font IDs for convenience.
Binary coordinates use the format's 16.16 quantization and text uses UTF-8 font
table declarations. A generator self-check decodes every binary drawing and
compares its complete element inventory and label/caption text against its XML.
This checks construction consistency, not independent application compatibility.

The currently published [DTD](https://static.chemistry.revvitycloud.com/cdxml/CDXML.dtd) has old naming discrepancies: its curve
attributes say `HeadCenterSize`/`HeadWidth`, while native modern files and this
repository use `ArrowheadCenterSize`/`ArrowheadWidth`. The corpus avoids those
optional curve sizing attributes. The DTD's atom `AS` declaration also contains
enum tokens (`+` and `?`) that XML DTD parsers reject. Consequently generation
does not claim DTD validation; well-formedness and binary inventory/text checks
are construction checks, while application imports provide compatibility evidence.

Primary format references: [graphics](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/Graphic.htm),
[curves](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/Curve.htm), [bracketed groups](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/BracketedGroup.htm),
and [embedded objects](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/EmbeddedObject.htm). Current application limits are
documented in `docs/graphics.md`; unsupported cases include shadows, doubled
curves, ordinary-graphic transparency, missing oval axes
and vector-only embedded pictures.
