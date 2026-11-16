# Inherited ChemDraw text styles

Synthetic inputs retain the encoder failure discovered while fixing document
`LabelFace` inheritance. The CDXML is hand authored. The independent reference
encoder writes zero-style CDXString runs so both the atom label and caption
inherit their document defaults. No ReShiki exporter constructs these inputs.
Regenerate the binary fixtures with `python3 generate.py` in this directory.

`defaults.cdx` is a complete drawing. `defaults.cds` is a full stationery
payload containing the same artwork plus packed document font defaults. A CDS
style loader reads document settings independently of the drawing importer.

The oxygen label must retain Times New Roman 18 pt bold italic, and the caption
must retain Arial 12 pt italic. Production CDX/CDS encoding formerly replaced
their unstyled spans with hard-coded Arial 10 pt plain runs. The manifest
records provenance, expectations, and input hashes.

`unused-palette.cdxml` retains a second production regression exposed by the
unchanged scene oracle. Its color table contains an unused red channel of 1.1;
the two carbon atoms have no text style and their bond explicitly uses built-in
black (index 0). The raw scene and complete drawing must retain both atoms and
the bond without resolving an unused default text-color entry. Actual text
styles still pass their existing validation and document color bounds.

This control has no CDX/CDS counterpart: binary color tables store normalized
unsigned 16-bit channels, so 1.1 cannot be represented. The independent encoder
refuses it; the manifest records the limit. The source channel is never clamped,
stripped, or wrapped merely to produce a binary companion.

Format reference: the published [CDXString definition](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/CDXString.htm).
