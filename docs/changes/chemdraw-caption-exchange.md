# Keep captions as text when exchanging with ChemDraw

Copying a drawing with formula captions into ChemDraw could create unwanted
chemical structures from those captions. Copying the complete drawing back
then failed because the generated formula fragments had invalid valences.
Numeric caption line heights also used the wrong units in binary CDX, causing
ChemDraw to display and save them at a fraction of the requested spacing.

The writer now marks captions with `InterpretChemically="no"` and retains that
explicit false value in CDX. Chemical atom labels keep their normal meaning.
The codec converts numeric line heights between CDXML points and CDX
twentieths of a point, rounding to the nearest representable step. The special
variable/automatic values are unchanged. No molecular validation is relaxed.

The actual ChemDraw 26.0.0.6599 captures in
[the fixture directory](../../tests/fixtures/chemdraw-captions/README.md) establish
both failures and the corrected exchange. The complete malformed original
remains an expected rejection. The corrected ChemDraw-written CDXML and CDX
preserve all six molecules, four editable protecting-group definitions, and
twelve text captions: 117 atoms, 123 bonds, and independently verified formula
`C108H144O3Si6` / InChIKey `BZUKRNRSTKNNSD-UHFFFAOYSA-N`.

Actual patched ReShiki → macOS clipboard → ChemDraw → macOS clipboard →
patched ReShiki verification passed for the complete gallery. The saved native
return preserved each molecule's independent identity, Si/O attachment,
editable group and reverse label. After translation, all atom positions,
including hidden group members, differ by less than 0.000041 points. All twelve
caption texts and complete font styles survive. The checked-in success files
are actual ChemDraw GUI saves from this application clipboard transfer, and
their provenance includes the independently analyzed native return.

ChemDraw quantizes the 10.8-point formula-caption line height to 10.75 points
when saving. Caption origins also shift by up to 1.833 points because ChemDraw
replaces layout bounds with ink bounds; this existing placement limitation is
separate from chemical interpretation and line-height units. No pixel-identical
caption positioning is claimed.

Review evidence uses the actual original and returned native drawings and
ChemDraw file captures. A screenshot cannot establish hidden formula atoms,
Si/O attachment, independent chemical identity, or binary line-height units;
those are verified from the captured documents. There are no renderer changes.
The complete editable [source gallery](../../tests/fixtures/chemdraw-captions/source.rsk)
and actual [clipboard return](../../tests/fixtures/chemdraw-captions/clipboard-return.rsk)
are included for reproduction. Other tested numeric-transform captions showed
an existing vertical origin shift of up to 1.9 points after ChemDraw exchange.

Validation includes explicit false-property bytes, numeric and special line
heights, formula and plain caption export, genuine ChemDraw file import,
existing typography/clipboard exchange, and the full independent codec and
drawing-writer differential corpora. The historical Python worker is unchanged;
the comparison harness accounts explicitly for these two deliberate format
corrections, using the independently captured byte values as evidence.

Previously exported CDX files with incorrectly encoded caption heights may
still fail paragraph validation. Re-exporting the native drawing with this fix
produces correctly encoded files; the importer does not guess or clamp those
ambiguous old values.

Release caption: **Keep formula captions as editable text and preserve their
line spacing when copying drawings between ReShiki and ChemDraw.**
