# Crystal structures

ReShiki opens crystal and periodic structures as a 2D drawing in their unit
cell, and shows them as a
[3D model](3d-keyboard-drawing.md#show-a-molecule-as-a-3d-model) with the same
styles, panel and **Show as** menu as any molecule shown in 3D.

## Open a structure

Choose **Open**, or drop the file on the window or use **Import → Choose
file…** to insert it into the drawing. These files are read:

| File                         | Read                                                                                                 |
| ---------------------------- | ---------------------------------------------------------------------------------------------------- |
| `.cif`                       | Cell, fractional atom sites, symmetry operators, disorder groups, displacement parameters (Uᵢⱼ, Bᵢⱼ) |
| `.pdb`                       | `CRYST1` cell and `ATOM`/`HETATM` records of the first model, for small molecules and ligands        |
| `POSCAR`, `CONTCAR`, `.vasp` | VASP 5 lattice and scale factor, element and count lines, Direct or Cartesian positions              |
| `.xyz`                       | A molecule without a cell                                                                            |
| `.xyz`, `.extxyz` (extended) | `Lattice="…"` and the `species` and `pos` columns of `Properties`                                    |

A structure with a cell opens as a 2D drawing with depth, facing the cell as
crystallographers draw it: _b_ to the right, _c_ up and _a_ toward you, seen a
little from above, in a parallel projection so the cell's edges stay parallel.
A skewed cell, such as a rhombohedral primitive cell, is tilted further so no
axis is seen end-on, and where atoms would hide behind each other or an axis
label, as rutile's Ti behind O, the nearest tilt that keeps every label apart
is taken. A plain XYZ file without a cell opens as a 3D model.

**Show as 3D** shows the model in that same view: drag inside the frame to turn
it, double-click to come back to the opening view, and use **Properties → 3D
model** for **Ball & stick**, **Stick**, **Wireframe** or **Space-filling**.
**Show as 2D** brings back the opening drawing, or **From the 3D view**
flattens the model as it is turned; **Both** puts the model beside the
drawing.

## What you see

- **The unit cell** is a box in the canvas's ink: the three edges hidden behind
  the cell are dashed, a dot marks the origin, and _a_, _b_ and _c_ label the
  lattice vectors. In 3D the box turns with the model, appears in every style
  and the model's frame always holds it. In 2D it follows the drawing's atoms
  wherever they are moved, and atom labels sit on a halo of paper so no edge
  runs through them. Both appear in PNG, SVG and PDF export.
- **The 2D drawing** shows atom labels and bond lines. Carbons with bonds are
  plain vertices, as in any skeletal drawing, and a bond behind another shows a
  gap where they cross.
- **Ion charges** are hidden in 2D, since the formula carries them; choose
  **Properties → Structure → Ion charges → Show** to label each ion, as Na⁺
  and Cl⁻. The choice is kept in the file.
- **Molecular crystals** keep whole molecules. Each molecule is made whole
  across the cell boundary and kept once, where its centre lies in the cell, so
  atoms of a molecule on a corner or face stick out of the box.
- **Extended networks** such as NaCl, rutile or silicon, where bonds link an
  atom to its own copy in the next cell, keep each atom once inside the cell.
  A bond that leaves the cell is drawn from its atom as a dashed half-bond
  toward the neighbouring cell.
- **Bonds** come from covalent radii (Cordero et al., 2008): two atoms are
  bonded when they are closer than their radii plus 0.45 Å, across the cell
  boundary too. In whole molecules, uncharged C, N, O and S take double or
  triple bonds until each has its usual valence, so CO₂ is O=C=O and urea has
  its C=O; a molecule where that does not work out exactly, as one whose file
  leaves out hydrogens, keeps single bonds, and so do extended networks. In a
  structure with oxygen, nitrogen,
  carbon, sulfur, phosphorus or a halogen, two metal atoms are never bonded, so
  rutile's Ti–Ti at 2.96 Å is not drawn as a bond; in metals and alloys they
  are.
- **Atoms** are explicit: no hydrogens are added, and a charge in a CIF type
  symbol (`Na1+`, `O2-`) or a PDB charge column stays on the atom.
- **Properties → Molecular properties** gives the formula and mass of the
  atoms shown (of the selected atoms, if any) and says that chemical analysis
  is unavailable, since the bonds come from distances, not chemistry.

## What is kept

A saved `.rsk` file keeps only what the structure needs: the lattice vectors,
each atom's element, position and charge, the bonds, the bonds that leave the
cell, and from the file each atom's label and displacement parameters. ADPs are
kept as U in Å² (B values are divided by 8π²), anisotropic U as CIF writes
them, for thermal ellipsoids in a later version. Everything else in the file —
bibliography, reflections, other data blocks, other XYZ properties or PDB
records — is read past and not kept, so a saved drawing stays small.

Saving a crystal needs document version 29. Every file is now saved as version
29, so ReShiki versions that read up to 28 cannot open it.

## Limits

- **CIF:** the first data block with a cell is read. Atoms come from
  `_atom_site_fract_x/y/z`; Cartesian `_atom_site_Cartn_*` sites are not read.
  Symmetry comes only from `_space_group_symop_operation_xyz` or
  `_symmetry_equiv_pos_as_xyz`; a file that lists none shows its listed atoms
  and says so, unless its space group is P1. Of disordered sites, atoms without
  a disorder group and those of the lowest group of each disorder assembly are
  kept. Atoms with an occupancy of 0 and dummy atoms
  (`_atom_site_calc_flag dum`) are skipped. Atoms closer than 0.25 Å to one
  already kept, such as the copies of an atom on a symmetry element, are the
  same site.
- **PDB:** symmetry copies (`REMARK 290`, `SMTRY`) are not generated, so the
  file shows the atoms it lists. Of alternate locations, the first of each
  residue is kept. A
  `CRYST1` of 1 × 1 × 1 Å, as NMR files write, means no cell. `ANISOU` records
  are not read; the B factor is kept as U.
- **POSCAR:** VASP 4 files without an element line are refused. A single
  negative scale factor is the cell volume, and three factors scale x, y and
  z.
- **Extended XYZ:** `pbc="F F F"` makes a molecule without a cell; a partly
  periodic structure, such as a slab, is refused.
- A cell thinner than 0.5 Å across a face, a CIF with more than 192 distinct
  symmetry operators, or one whose sites times operators exceed 40,000, is
  refused.
- A cell so thin or skewed that a bond reaches a copy more than three cells
  away is refused with a request for a reduced (Niggli) cell.
- A structure may have at most 2,000 atoms in its cell. Whole proteins, CIF
  export, editing the lattice and space-group analysis are out of scope.
- If anything in the file fails validation, nothing is opened, and the message
  names the line where it can.

Repeating the cell with ghost copies and looking along _a_, _b_, _c_ or [111]
are planned ([issue #417](https://github.com/Ameyanagi/ReShiki/issues/417)).

## Implementation notes

`crates/io/src/crystal.rs` and its modules read each format into atoms at
Cartesian positions and an optional lattice, wrap periodic atoms into the cell
and build the drawing (`crystal/build.rs`). Bonds are found between every pair
of atoms and their lattice copies within reach. The boundary rule walks each
connected group, giving each atom a cell offset; if a bond reaches an atom at a
different offset than the walk gave it, the group is periodic (a network) and
its atoms stay wrapped, with each bond to another cell recorded as a
`models3d::Crossing` (atom, partner, cell offset). Otherwise the group is a
molecule: it is unwrapped by those offsets and moved by whole cells so its
centre is inside.

The model (`crates/model/src/models3d.rs`) stores the lattice in
`Model3d::cell` (`models3d::crystal::Cell`) and the file's labels and ADPs in
`Model3d::sites`; a model read from a file may hold several whole molecules.
Model positions are Cartesian ångström with the cell's origin at (0, 0, 0), so
a copy in another cell is the position plus whole lattice vectors, which later
repeats and ghost copies use. In 3D the scene draws the hidden cell edges as dashed
`Wire` primitives before the model, the front edges, origin and labels after
it, and the half-bonds leaving the cell in depth order with the atoms. Labels
are ordinary text primitives, so the canvas and exports turn them light on a
dark canvas as they do atom labels.

In 2D, `models3d::flat_cells` fits a parallel projection from the model's
positions to the 2D atoms (least squares, held toward the model's view where
the atoms span no volume) and draws the same box through it: hidden edges and
half-bonds before the bonds, front edges after them, and the axis labels after
the atom labels, which get a paper halo. `models3d::drawn` drops the charges
of a crystal whose `Model3d::charges` is off, only for painting. The opening
view (`crystal_rotation`) tries tilts nearest the standard one until labelled
atoms (all but bonded carbons) and the axis labels are about a label's height
apart, for up to 300 such atoms. Bond orders come from `crystal/build.rs`.

Test fixtures in `tests/fixtures/crystal/` are hand-written from published cell
parameters: NaCl (Fm-3m), dry ice CO₂ (Pa-3), urea (PDB), rutile TiO₂
(POSCAR), water (XYZ) and silicon (extended XYZ). The CO₂ displacement
parameters are illustrative.
