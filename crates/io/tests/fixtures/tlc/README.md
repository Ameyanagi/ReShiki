# Original TLC plate fixtures

The project owner authored these files in ChemDraw 26.1.0.6801 during the local
ChemDraw interoperability review on 2026-11-21. They are original test artwork,
not examples copied from a third-party catalogue.

- `chemdraw26-mixed-spots.cdxml` and `.cdx` are the original paired saves of
  `TLC plate mixed spots`: three lanes with Rf values 0.28, 0.51 and 0.77.
  The first spot shows its Rf label; the first two spots are filled and the
  third is hollow. The associated positioned label has a subscript f.
- `chemdraw26-default.cdxml` is the original `TLC plate default specimen`:
  three filled spots at Rf 0.15, without Rf labels.
- `chemdraw26-reaction-check.cdxml` and `.cdx` are the owner's original
  `TLC reaction check`, reopened and saved by ChemDraw SDK 26.8.0.6801. The
  240 × 200 pt plate has three lanes, four spots at Rf 0.67/0.32 and one
  shown Rf label. Its native zero-length show flags reproduce the binary
  writer's SDK error 536 when incorrectly replaced by one-byte booleans.

The four plate corners, reference lines, physical spot sizes and authored label
runs are intentional regression data. Fixture provenance and expected physical
geometry come from the owner's actual ChemDraw save and matching ChemDraw SDK
render. No user-name fields or home paths are included.
