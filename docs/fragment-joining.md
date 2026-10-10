# Move and attach existing fragments

Select an atom or molecule and choose **Move & attach…** above the canvas.
The entire connected fragment moves, so selecting one endpoint never cuts its
other bonds. Explicitly selected captions/graphics and integral groups move too.

Use the inspector preview to choose the exact source atom or bond:

- **Connect with a bond** joins the chosen source atom to a destination atom with
  a new single bond, aligning both attachment directions.
- **Share an atom** merges compatible source and destination atoms.
- **Fuse along a bond** shares the chosen source bond's atoms with a compatible
  destination bond. Choosing a source bond switches to this mode.

Hover a destination to preview. Click to join, or drag from the destination to
choose direction/side. Shift/Ctrl snaps the direction; Alt releases it. The
preview and final join use the same placement geometry. Escape, Cancel or
switching tools leaves the original drawing intact. Joining is one Undo step.

Moved objects retain their IDs; shared atoms retain the destination IDs. Groups
and captions are reconciled, unrelated objects stay fixed, and remote stereo
references survive remapping. A document revision/file guard prevents an old
preview from overwriting newer edits. Assistant application waits until an
active joining gesture finishes or is cancelled.

The destination must be a separate existing fragment. The same chemistry
restrictions as template attachment apply: compatible element, bond order,
valence and stereo at the attachment site. Expand abbreviations before using
their anchors. Common five/six-member aromatic fusion can reassign Kekulé bonds;
this is not an unrestricted graph merge or general stereocenter-merging tool.

`tests/joining.rs` checks connect/share/fuse identities, stable IDs, captions,
groups, single-atom/edge merging, remote tetrahedral stereo and rejected input.
App tests check cancellation, mode selection, stale previews and atomic Undo/Redo.
A 2026-09-20 desktop check selected an existing furan edge, fused it onto benzene,
verified Undo/Redo and saved the resulting nine-atom, ten-bond drawing. Reopening
and chemistry analysis produced benzofuran (C8H6O).

## Coordination contacts in place

The [native desktop validation](coordination-desktop-validation.md) records
ordinary rejection, in-place contacts, labeled keyboard controls, projection-tip
reversal and saved-file graph checks. The contribution remains under review.

Select a donor atom or ligand and choose **Coordinate in place…** above the
canvas or in its context menu. You can also choose **Coordinate in place
(donor → metal)** in the Move & attach mode list. Choose the donor in the
preview, then click the existing metal atom. Neither atom moves or disappears.
Repeat for the second donor of a chelate, even when both are already in the
same connected molecule. Each new contact is one Undo step; Escape cancels
without editing. Repeating an existing donor→metal contact is a no-op.

For a selection shortcut, select exactly the donor atom and the metal, then
press **Ctrl+J** (**⌘J** on macOS). Either selection order creates the same
donor→metal contact without moving or merging atoms. Repeat with the other
donor to close a chelate; an existing contact adds no Undo step. Select the
two endpoint atoms rather than the whole ligand. For two ordinary nonmetal
atoms, this shortcut still shares compatible atoms; four ordinary bond
endpoints still fuse the bonds.

The [selected-pair shortcut check](changes/coordination-join-shortcut.md) records
matched native before/after, field-focus protection and Save/reopen controls.

The explicit operation supports neutral/−1 charged N, O, S and P lone-pair
donors and transition-metal acceptors Sc–Zn, Y–Cd and Hf–Hg, with up to 12
contacts at a metal. Unsupported donor valence, radicals, atom stereo and
unexpanded groups give a specific diagnostic. This drawing workflow does not
predict stability, coordination geometry, oxidation state or Δ/Λ stereo.
It retains entered charge and hydrogen metadata. Ordinary **Connect with a
bond** keeps its covalent valence checks, and **Share an atom** still merges
compatible atoms.

For marked keyboard drawing, mark the donor with **[**, navigate to the metal,
then choose **Coordinate }** or press **}** (Shift+] on a US keyboard).
**]** remains an ordinary single-bond connection. See
[keyboard drawing](3d-keyboard-drawing.md).

An existing directed dative contact can use Solid wedge, Hashed wedge, Hollow
wedge, Bold or Hashed paint from the bond Style control. These styles depict
projection and preserve chemical order 5 and donor→metal direction; they do
not assign tetrahedral or complex stereochemistry. Changing back to Dative
or Coordination (dashed) retains the same direction. A reversed drawing
gesture does not reverse that chemical direction. Choose **Reverse projection tip**
in Crossings & direction to switch between donor- and metal-narrowed wedge/hash
paint. This changes the tip appearance without swapping chemical endpoints.

Native ReShiki and supported CDXML retain charge, direction and projection
paint. CDXML writes explicit `Order="dative"` with donor B and metal E; a
wedge can be narrowed at its donor (Begin) or metal/acceptor (End), independently
of the chemical direction. CDX preserves the same order/end/display properties.
This retains the projection attributes for ReShiki roundtrips. Actual ChemDraw
26.0.0.6599 review of the paired Co(en)3 fixture displayed dative contacts as
arrows, ignoring wedge/hash Display attributes, and removed those attributes
on save. Its graph still retained all donor→Co contacts, NH2 and Co3+.
Use native ReShiki or SVG/PDF/PNG for faithful coordination projection paint;
CDXML/CDX cannot guarantee that appearance in ChemDraw.
Older ReShiki builds reject projected order-5 styles
at validation instead of silently reading them as covalent stereo.
MOL export retains directed dative type 9 in V3000 and charge, but omits
projection paint and may redraw coordinates for chemical stereo; the export
reports this limitation. Use native/CDXML or figure export for the drawing.

Direction and donor-valence semantics follow the
[RDKit Book](https://github.com/rdkit/rdkit/blob/master/Docs/Book/RDKit_Book.rst).
Revvity also documents unified coordination/dative behavior in its
[official release notes](https://revvitysignals.com/whats-new/all?sort_order=ASC).
