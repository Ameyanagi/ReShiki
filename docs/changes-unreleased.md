# Unreleased changes

SVG files look the same in every viewer. **Export → Figure → SVG** and the
agent tools `export`, `file_save` and `render` now draw text as outlines, as
`reshiki --cli` and the Python package already do. Live `<text>` used the
viewer's own fonts, so labels shifted between apps, and a renderer without
Arial could drop them. To keep text editable, turn on **Keep text editable**
below the export button (off by default; ReShiki remembers it) or pass
`svg_text: "live"` to the tools. An editable drawing inside the SVG still
reopens in ReShiki. **SVG for Office** is unchanged.
[Text in SVG files](figure-export.md#text-in-svg-files).

SMILES like `[C]` and `[Si]` now import as written: a bare carbon or
silicon atom with four unpaired electrons, and `[CH]`, `[N]` and `[B]` with
three, as RDKit reads them. Earlier versions added hydrogens to keep at most
two unpaired electrons and said so in a warning, which changed the formula,
mass and InChI (`[C]` came in as CH₂). The formula, mass and InChI now match
the input (`[C]` is C, 12.011, `InChI=1S/C`), SMILES and MOL export keep these
atoms exactly, and RDKit's MOL files for them open. Drawing a bond to such an
atom recounts its unpaired electrons instead of reporting an error. CDXML
cannot store three or four unpaired electrons, so ChemDraw files keep the
hydrogen count and show a doublet or triplet. Drawings with these atoms save
in a newer document format that earlier ReShiki versions report they cannot
open. [Scientific symbols](scientific-symbols.md#persistence-and-exchange).

See [ReShiki 0.14.0](changes-0.14.md) for the prepared release notes and
[release validation](release-0.14.0-validation.md) for publication status.
