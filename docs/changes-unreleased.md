# Unreleased changes

On Linux, **Copy** (**Ctrl+C**) also puts the selection's SMILES on the
clipboard as plain text, the same text **Copy SMILES** gives, so a plain-text
box gets the SMILES. LibreOffice, Chromium pages and ReShiki still paste the
drawing or picture, and when ReShiki quits with its window focused, GNOME still
keeps the picture. A reaction, or a selection without molecules, gets no text.
Windows and macOS are unchanged, because Excel and PowerPoint there would paste
the text instead of the drawing.
[PR #1161](https://github.com/Ameyanagi/ReShiki/pull/1161) ·
[Clipboard compatibility](clipboard-compatibility.md).

SVG files look the same in every viewer. **Export → Figure → SVG** and the
agent tools `export`, `file_save` and `render` now draw text as outlines, as
`reshiki --cli` and the Python package already do. Live `<text>` used the
viewer's own fonts, so labels shifted between apps, and a renderer without
Arial could drop them. To keep text editable, turn on **Keep text editable**
below the export button (off by default; ReShiki remembers it) or pass
`svg_text: "live"` to the tools. An editable drawing inside the SVG still
reopens in ReShiki. **SVG for Office** is unchanged.
[Text in SVG files](figure-export.md#text-in-svg-files).

The SVG that ReShiki puts on the clipboard draws its text as outlines on
macOS, Linux and in the browser editor, as Copy Image on Windows already did.
**Copy**, **Copy Image** and **Copy as → SVG** used live `<text>` there, so an
app that pastes the SVG flavor substituted its own fonts. The SVG keeps its
physical size, transparent background and the editable drawing inside it.
[Clipboard](clipboard.md).

Generated names follow the IUPAC 2013 recommendations more closely, with the
naming engine updated to [nazuchem 0.2.0](https://github.com/Ameyanagi/nazuchem/releases/tag/v0.2.0).
Symmetric structures get multiplicative preferred names, such as
`1,1'-oxybis(4-bromobenzene)` and `4,4'-(propane-2,2-diyl)diphenol`. Salts list
their ions alphabetically, ignoring stereo, isotope and italic descriptors
(`disodium tert-butoxide methoxide`). A charged ring or nitrogen becomes the
parent (`4-(hydroxymethyl)-1-methylpyridin-1-ium`), and charged substituents use
the preferred prefixes `(N,N-dimethylmethanaminiumyl)` and
`diazyn-1-ium-1-yl`. Isotope labels inside a substituent are named, as in
`(2H3)methoxybenzene`, where ReShiki refused before. Diamines cite `N^1` and
`N^3`, a nitrile in a substituent is `cyano`, ring ketones use indicated
hydrogen (`1,3-dihydro-2H-1,4-benzodiazepin-2-one`), and parentheses and
fusion locants are simpler (`hydroxy(phenyl)acetic acid`,
`2H-benzo[cd][1]benzothiophene`). [2.2]Paracyclophane now shows a von Baeyer
name with its phane name as the alternative, like other cyclophanes.

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
