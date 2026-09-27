These CDXML inputs come unchanged from `External/ChemDraw/test_data/` in RDKit
commit `0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985` (the pinned 2026.03.6 source).
They exercise explicit hydrogen policy, radicals, tetrahedral drawing geometry,
atropisomers, and the ChemDraw double-bond stereo override.

The reference test calls `Chem.MolsFromCDXML` with `sanitize=False` and
`removeHs=False`. `atom-to-fragment.cdxml` requires abbreviation expansion
before this molecular-reader boundary, so its rejection is recorded separately
from native parity. Annotation metadata is accepted by the molecular reader;
`geometry-tetrahedral-4.cdxml` is compared directly against the native reader,
including its atoms, bonds, positions and stereochemistry.

The complete scene/import tests separately cover the newly accepted annotated
`atom-to-fragment.cdxml` drawing using direct RDKit canonical SMILES, atom/bond
counts, and retained abbreviation membership. The legacy Python importer rejects
this annotation/multiple-attachment combination; it is not used as the oracle
for that one accepted extension.

RDKit source licensing: see `licenses/rdkit/LICENSE` and `licenses/rdkit/NOTICE`.
