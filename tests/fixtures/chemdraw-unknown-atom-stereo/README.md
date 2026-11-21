# Unknown atom stereochemistry in ChemDraw CDX

`source.cdxml` is an original five-atom dimethylcarbodiimide drawing. It has
no assigned atom stereochemistry and no `AS` attributes. It contains no copied
vendor drawing data.

`native.cdx` is ChemDraw SDK 26.8.0.6801's binary save of that source. The SDK
writes `AS="?"` on its central atom, ID 4. Property `0x0437` at offset 1178
has one byte, `0c` (12). The paired SDK CDXML writes the question-mark spelling.
File and SDK executable checksums are in [provenance.json](provenance.json).

The same spelling occurs in the external Solid-phase Peptide Synthesis corpus.
Its binary route previously refused the whole drawing as an unsupported `AS`
enumeration, although its CDXML route was readable. This fixture reduces the
codec difference without copying that example.

The codec preserves the question mark through CDX/CDXML conversion. It does not
assign R/S or add axial-stereochemistry support to the molecular model. Other
unknown enum values still fail, and existing `AS="N"` metal-projection rules
remain unchanged.
