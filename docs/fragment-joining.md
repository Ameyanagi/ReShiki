# Move and attach existing fragments

**Join** connects two fragments at the atoms or bonds you select, or merges
three or more selected atoms into one. **Move & attach…** moves a whole fragment
onto another, and **Coordinate in place…** adds a donor → metal bond without
moving anything.

## Use it

### Join selected atoms or bonds

1. Select the atoms. Shift-click adds atoms in the order you want; the first
   atom you select is kept when atoms merge.
2. Choose **Join** in the context row, or right-click and choose **Join
   selected atoms or bonds** (Ctrl+J, ⌘J on macOS).

| Selection                                      | Result                                             |
| ---------------------------------------------- | -------------------------------------------------- |
| One atom in each of two fragments              | The fragments share that atom.                     |
| Both ends of one bond in each of two fragments | The fragments fuse along the bond.                 |
| Three or more atoms                            | The atoms merge into one. Nothing else moves.      |
| A donor atom and a metal                       | A donor → metal bond is added. Neither atom moves. |

Join merges three or more atoms into the first atom you selected. The merged
atom sits at the center of the selected atom labels and keeps the first atom's
ID, element, label color and style. Surrounding atoms and captions stay in
place. Merging three or more atoms is available in Nightly builds; not in
ReShiki 0.11.0.

Four atoms that form two bonds fuse the bonds. To merge all four atoms into one
instead, choose **Merge atoms** in the context row, or **Merge selected atoms**
in the right-click menu.

### Move a fragment onto another

1. Select an atom or molecule and choose **Move & attach…** above the canvas.
   The whole connected fragment moves, together with selected captions,
   graphics and groups.
2. Pick the attachment atom or bond in the inspector preview, and choose how it
   connects:
   - **Connect with a bond** adds a single bond to a destination atom.
   - **Share an atom** merges the attachment atom with a matching atom.
   - **Fuse along a bond** shares the attachment bond's two atoms with a
     matching bond. Picking a bond in the preview selects this mode.
3. Hover a destination to preview the result. Click to join, or drag from the
   destination to choose the direction. Shift or Ctrl snaps the direction; Alt
   releases it.

Escape, **Cancel** or switching tools leaves the drawing unchanged. Each join is
one Undo step.

### Coordination contacts in place

Available in Nightly builds; not in ReShiki 0.11.0.

1. Select a donor atom or ligand and choose **Coordinate in place…** above the
   canvas or in the right-click menu. **Coordinate in place (donor → metal)** in
   the **Move & attach** mode list does the same.
2. Pick the donor (N, O, S or P) in the preview, then click a metal on the
   drawing. Neither atom moves.
3. Repeat for each donor of a chelate, even when both donors are already in the
   same molecule.

You can also select exactly the donor atom and the metal and press **Ctrl+J**
(**⌘J** on macOS). Select the two atoms, not the whole ligand. In keyboard
drawing, mark the donor with **[**, move to the metal, then choose
**Coordinate }** or press **}** (Shift+] on a US keyboard). See
[keyboard drawing](3d-keyboard-drawing.md).

Each new contact is one Undo step, and adding a contact that already exists does
nothing. Donor hydrogens and the metal's charge stay as you drew them.

To show the contact as a wedge or hash, choose a style such as **Solid wedge**
or **Hashed wedge** in the bond **Style** control. **Reverse projection tip** in
**Crossings & direction** narrows the wedge at the other end. Clicking a plain
dative bond again with the **Dative** tool swaps donor and metal, the same as
**Reverse bonds**.

## Limits

- The two fragments must be separate. Join and Move & attach check element,
  bond order, valence and stereo at the attachment site, as template
  attachment does.
- Expand abbreviations before you join or merge at their atoms.
- A merge is rejected when the atoms have different charge, isotope, fixed
  hydrogens, radical, aromatic, map or mark states, or when it would merge
  stereocenters or bonds with stereo.
- Join placement and selection still need work for some cases
  ([issue #246](https://github.com/Ameyanagi/ReShiki/issues/246)).
- Coordinate in place works for neutral or −1 N, O, S and P donors and the
  transition metals Sc–Zn, Y–Cd and Hf–Hg, with up to 12 contacts per metal.
  It does not predict stability, geometry, oxidation state or Δ/Λ stereo.
- Wedge and hash styles on a contact only show the projection. They do not
  assign stereochemistry.
- ChemDraw shows coordination contacts as arrows and drops their wedge and hash
  styles. Use a `.rsk` file or SVG, PDF or PNG to keep the look.

## Implementation notes

### Multi-atom merge

Bond fusion requires four distinct endpoints. Two adjacent bonds among four
selected atoms use the multi-atom merge. The merged position is the center of
the selected atoms' visible label-ink bounds; an atom with a hidden label
contributes its position instead.

Merging removes internal self-bonds and combines identical duplicate bonds.
Conflicting bond metadata, charge, isotope, fixed hydrogen, radical, aromatic,
map or mark states are rejected with a diagnostic. Abbreviations must be
expanded first. Merging stereocenters or incident stereo bonds, collapsing
stereo neighbors, centroid/attachment target sets or painted rings, and
conflicting reaction roles or depth paint are also rejected. Compatible remote
stereo, groups, reaction coefficients and atom references are remapped. The
whole merge is one Undo step. Projection depth retains the mean of the selected
depths; the XY label-bounds placement does not establish a ChemDraw-compatible
3D rule. Headless model calls (`merge_atoms`) use the selected atom-position
bounds center unless supplied a finite position (`merge_atoms_at`).

### Move & attach

The preview and final join use the same placement geometry. Moved objects
retain their IDs; shared atoms retain the destination IDs. Groups and captions
are reconciled, unrelated objects stay fixed, and remote stereo references
survive remapping. A document revision/file guard prevents an old preview from
overwriting newer edits. Assistant application waits until an active joining
gesture finishes or is cancelled. Common five/six-member aromatic fusion can
reassign Kekulé bonds; this is not an unrestricted graph merge or general
stereocenter-merging tool.

The [original Move & attach checks](changes/move-and-attach-20260920.md)
record the first build's tests and desktop check.

### Coordination

For the Ctrl+J shortcut, either selection order creates the same donor→metal
contact without moving or merging atoms. For two ordinary nonmetal atoms, the
shortcut still shares compatible atoms; four ordinary bond endpoints still fuse
the bonds. Unsupported donor valence, radicals, atom stereo and unexpanded
groups give a specific diagnostic. Ordinary **Connect with a bond** keeps its
covalent valence checks, and **Share an atom** still merges compatible atoms.
**]** in keyboard drawing remains an ordinary single-bond connection.

A directed dative contact can use **Solid wedge**, **Hashed wedge**, **Hollow
wedge**, **Bold** or **Hashed** paint. These styles depict projection and keep
chemical order 5 and the donor→metal direction; they do not assign tetrahedral
or complex stereochemistry. Changing back to **Dative** or **Coordination
(dashed)** keeps the same direction, and a reversed drawing gesture does not
reverse it. **Reverse projection tip** switches between donor- and
metal-narrowed paint without swapping chemical endpoints.

`.rsk` files and supported CDXML keep charge, direction and projection paint.
CDXML writes explicit `Order="dative"` with donor B and metal E; a wedge can be
narrowed at its donor (Begin) or metal/acceptor (End), independently of the
chemical direction. CDX preserves the same order/end/display properties. This
retains the projection attributes for ReShiki roundtrips. ChemDraw 26.0.0.6599
displayed the paired Co(en)3 fixture's dative contacts as arrows, ignored their
wedge/hash Display attributes and removed those attributes on save. Its graph
still retained all donor→Co contacts, NH2 and Co3+. CDXML/CDX cannot guarantee
that appearance in ChemDraw. Older ReShiki builds reject projected order-5
styles at validation instead of silently reading them as covalent stereo. MOL
export retains directed dative type 9 in V3000 and charge, but omits projection
paint and may redraw coordinates for chemical stereo; the export reports this
limitation. Use a `.rsk` file, CDXML or figure export for the drawing.

Direction and donor-valence semantics follow the
[RDKit Book](https://github.com/rdkit/rdkit/blob/master/Docs/Book/RDKit_Book.rst).
Revvity also documents unified coordination/dative behavior in its
[official release notes](https://revvitysignals.com/whats-new/all?sort_order=ASC).
The [desktop validation](coordination-desktop-validation.md) and the
[selected-pair shortcut record](changes/coordination-join-shortcut.md) describe
the checks for this feature.
