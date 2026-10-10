# MOL coordinate and hydrogen import controls

These are original small fixtures generated locally with RDKit 2026.03.6;
they contain no downloaded experimental data. The source structures are
hexane (`CCCCCC`), `F[C@H](Cl)Br`, `F/C=C/F`, and
`F[C@]([2H])([3H])Cl`. The 3D variants use explicit hydrogens,
`ETKDGv3` with seed 61453, and at most 200 MMFF optimization iterations.
The files, rather than a regenerated conformer, are the coordinate reference.
The 2D controls use `rdDepictor.Compute2DCoords` without added ordinary H.
V2000 and V3000 contain the same conformer within their respective decimal
precision. The two reordered chiral controls move the explicit H to the
first or middle atom position while preserving molecular stereochemistry.
The unknown-double variants change only the double-bond flag to explicit ANY
(V2000 stereo3, V3000 CFG2). They must stay ANY after native save, rotation,
chemical preparation and MOL interchange, rather than becoming unspecified.
Additional controls cover the opposite chiral configuration, Z alkene,
ordinary H and D on the same stereocenter (`F[C@H](Cl)[2H]`), and explicit wavy
H single bonds. The latter two retain source H IDs through remapping or
protection. Native wavy markers survive rotations; the existing 2D MOL writer
may omit a wavy single marker while leaving chirality unassigned.

Exceptional controls were read from `[2H].[3H]`, `[H+].[H-]`, `[H-][B]`,
`[H]`, `[H][Fe]`, and `[Fe]<-[H]->[Fe]` with
`SmilesParserParams.removeHs=False` and written as V3000 after 2D depiction.
They cover isotope, charge, radical, isolated, terminal metal, and bridging
coordination H. These hydrogens must remain explicit and visible.

The independent reference command opens the files through the actual
executable's headless file API, retains native and MOL output plus receipts,
and checks graph identity using RDKit rather than ReShiki's parser:

```sh
.venv/bin/python reference/molfile_xyz_reference.py \
  --executable /absolute/path/to/reshiki \
  --output-dir /absolute/path/to/new-evidence-directory
```

Add `--baseline` only for the frozen pre-fix executable: it asserts the
original flattened XYZ and explicit-H presentation. Candidate checks expect
hexane to have six drawn carbons with implicit H, chiral/alkene controls to
retain their assigned configurations, and protected H to remain visible.
Native saves retain every kept atom's source XYZ under the existing single
canvas factor `(x, y, z) -> (28x, -28y, 28z)`, rounded to `f32`. The command
checks native reopen/save persistence, heavy-atom distances, isotope/charge/
radical properties, stable source IDs and directed coordination edges.
It also checks MOL export's molecular identity; that export deliberately uses
the existing stereo-safe 2D depiction and is not a source-XYZ round trip.
