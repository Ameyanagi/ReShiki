# Actual ChemDraw caption exchange captures

These are unmodified GUI saves from ChemDraw 26.0.0.6599 on macOS 26.5.1 arm64,
captured on 2026-09-29. `provenance.json` records hashes and exact capture paths.
The original drawing contains three copies each of two protecting-group
molecules, four collapsed definitions, and twelve captions (six descriptions
and six formula captions).

`source.rsk` is the original editable gallery; `clipboard-return.rsk` is the
actual native save after the corrected clipboard round trip. The ReShiki
0.9.1 debug build used combined integration base
`446331ec8ffdef3c852cccec8b13e2105d9e6737` plus the production compatibility
patch recorded by SHA-256 in provenance. It included other concurrent feature
UI; this fix changes exchange semantics, not the renderer.

- `gallery-before.cdxml`: actual ReShiki clipboard → ChemDraw paste → GUI save.
  ChemDraw interpreted each formula caption as an additional chemical fragment
  and marked it with an invalid-valence warning. The six description captions
  received line heights of 0.55 points. This complete failed return must remain
  rejected; the importer must not silently discard these extra fragments.
- `gallery.cdxml` and `gallery.cdx`: the same original drawing was copied from
  the patched ReShiki application through the macOS clipboard into ChemDraw,
  then saved through ChemDraw's GUI to both formats. These two
  **ChemDraw-generated** outputs preserve all twelve text captions and all six
  molecular graphs. The original prototype captures were superseded by this
  actual application clipboard test. A second actual clipboard copy back into
  ReShiki and native save also passed, with the native return hash recorded in
  provenance. The native implementation did not generate these expected files.

The file pair independently establishes CDX caption height 240 = CDXML 12 pt
and CDX 215 = CDXML 10.75 pt. The input fractional height was 10.8 pt; ChemDraw
rounded it down by one 0.05 pt step when saving. The native encoder rounds to
the nearest representable step rather than reproducing that downward bias.

Chemical expectations come from three copies each of these independently
specified reference SMILES:

```text
CC(C)(C)[Si](CC)(c1ccccc1)c1ccccc1
CC(C)(C)[Si](OCC)(c1ccccc1)c1ccccc1
```

Pinned RDKit 2026.03.6 independently gives combined formula `C108H144O3Si6`
and InChIKey `BZUKRNRSTKNNSD-UHFFFAOYSA-N`. There are 117 atoms and 123 bonds.
The four collapsed definitions retain their Si/O anchors and 17/18 members.

The [original ChemDraw SDK documentation](https://chemapps.stolaf.edu/iupac/cdx/sdk/properties/InterpretChemically.htm)
defines chemical interpretation as enabled when the property is absent. A
caption therefore needs an explicit false value. The archived line-height
documentation calls values screen units; the actual captured binary/XML pair
above supplies the independent evidence for the twentieth-point conversion.

The failed original binary was overwritten by ChemDraw autosave while the test
document was reused, so it is intentionally not included. An initial diagnostic
reported unsupported property `0x044b`; it does not occur as a decoder blocker
in the corrected capture. No speculative property mapping or validation
relaxation is part of this fix.
