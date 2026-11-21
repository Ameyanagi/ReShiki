# ChemDraw dipole arrow

`source.cdxml` is the single dipole arrow and its linked legacy graphic reduced
from the ChemDraw 26.8.0.6801 `p5-arrows-enum` probe. It is editable arrow data,
with tail `(130, 265)` and head `(205, 265)`, rather than a shape fallback.

`native.cdx` is ChemDraw SDK 26.8.0.6801's binary save of that reduced CDXML.
Its arrow object is `0x8021`, and its `Dipole` property `0x0a3a` has an empty
payload. The original full ChemDraw CDX probe uses the same empty flag.
File and SDK executable checksums are in [provenance.json](provenance.json).

ReShiki previously wrote a one-byte Boolean (`3a 0a 01 00 01`). ChemDraw's CDX
reader refused it with SDK error 536. Changing only that property to an implied
Boolean (`3a 0a 00 00`) lets ChemDraw reopen and resave the arrow, keeping
`Dipole="yes"`. Text CDXML already reopens correctly.

The tests lock the binary representation, keep old one-byte ReShiki flags
readable, omit a false flag, and retain the arrow's dipole meaning and endpoints.
