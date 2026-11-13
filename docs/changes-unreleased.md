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

See [ReShiki 0.14.0](changes-0.14.md) for the prepared release notes and
[release validation](release-0.14.0-validation.md) for publication status.
