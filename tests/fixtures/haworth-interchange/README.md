# ChemDraw Haworth references

These 14 drawings were generated from ReShiki's atom/bond graphs and saved by
ChemDraw 26.0.0.6599 on macOS on September 24, 2026. They cover the ten named
α/β D-sugars, two oxygen-ring scaffolds and two carbon-ring outlines. CH₂OH is
expanded to carbon and oxygen in these input references.

The input CDXML contains coordinates and bond displays, without cached `AS`,
`Geometry` or `BondOrdering` attributes. ChemDraw supplies its own interpretation
when saving. `manifest.json` records its SMILES and independent reference
InChIKeys. CDXML, CDX and both MOL versions are kept here for regression tests.

The complete 26-format export and 10-option clipboard study is generated under
`artifacts/haworth-interchange/corpus`, outside the repository. Capture and verify
with `scripts/chemdraw_haworth_corpus.py` and
`scripts/chemdraw_haworth_verify.py`. See
[the study documentation](../../../docs/haworth-interchange.md).

These are our test drawings saved through the application, not copies of its
proprietary template libraries. Actual file sizes and hashes are recorded in the
generated report. Cached CIP letters are not treated as stereochemical proof.
