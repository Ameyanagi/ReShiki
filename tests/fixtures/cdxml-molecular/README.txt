These CDXML inputs come unchanged from `External/ChemDraw/test_data/` in RDKit
commit `0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985` (the pinned 2026.03.6 source).
They exercise explicit hydrogen policy, radicals, tetrahedral drawing geometry,
atropisomers, and the ChemDraw double-bond stereo override.

The reference test calls `Chem.MolsFromCDXML` with `sanitize=False` and
`removeHs=False`. `atom-to-fragment.cdxml` and `geometry-tetrahedral-4.cdxml`
contain annotation objects that the current application rejects; the test
records their rejection separately from native parity. The first also requires
abbreviation expansion before this molecular-reader boundary.

RDKit source licensing: see `licenses/rdkit/LICENSE` and `licenses/rdkit/NOTICE`.
