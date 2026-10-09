# Align and stretch a reference bond or edge

Select both ends of a bond, or select a drawing shape, then open **Properties →
Transform → Reference: align / stretch**. Choose the reference in the list.
**Connected fragment** carries the molecule containing the selected bond;
**Selected objects** acts only on complete selected fragments and drawing objects.
A partial molecule is rejected rather than stretching its exterior bonds.

**Horizontal** and **Vertical** rotate the reference to the nearest matching
axis, using its actual angle. **To angle** uses a directed angle: 0° points right,
90° down, 180° left and −90° up. These operations accept arbitrary orientations;
they do not round the reference to a 15° grid. A wedge's reference is its bond
axis, retaining its endpoint order, taper and stereochemistry. Alignment rotates
the drawing in its plane and leaves projection depth unchanged.

Choose **Center of moved objects**, **Reference start**, **Reference midpoint** or
**Pinned point** as the pivot. The first option uses the stable center of all
objects in the chosen scope. **Pin selected center** captures the current
selection's stable center. Its X/Y fields are publication points in the drawing's
coordinate system. The pin stays fixed across edits and selection changes until
changed or another file is opened. **Turn** is a relative rotation around this
pivot; **Copy + rotate** leaves the originals and selects the rotated copy. For
fourfold construction, retain one common center and make quarter-turn copies of
the original fragment at 90°, 180° and 270°. Shared coordinates do not automatically
connect atoms; joining and chemical bond assignment remain explicit operations.

The existing numeric Rotate field and keyboard commands retain their relative
rotation behavior. The ordinary rotation handle remains free by default; Shift
continues to snap that gesture to 15° steps.

## Stretch only the chosen bond length

For a bond separating two fragments, **Set length** changes its length in points.
**Drag to stretch** enters a mode that keeps the exact original bond direction,
even at angles such as 17.3°. The square marker is the fixed end and the round
marker the moving end. Drag the moving end or its branch; perpendicular pointer
motion is ignored. **Swap fixed end** chooses the other side. All atoms downstream
of the moving end translate together, retaining their internal lengths and angles.

The length cannot cross zero: dragging clamps to 0.1 pt. Stretching never merges
nearby atoms or fuses rings. A ring edge cannot generally change only its length
while all other ring geometry stays rigid, so it does not offer this operation.
Additional fixed attachments and semantic attachment nodes are also rejected.
Use ordinary selection movement for broader geometry changes.

Each committed alignment, rotation, copy or stretch is one Undo step; an unchanged
alignment records none. Escape or loss of focus cancels a stretch gesture, and
release outside the canvas leaves the drawing unchanged. **Done** returns to
Select. Switching tabs or replacing the drawing also ends stretch mode; choose
the reference again in the new drawing. Native files retain the resulting coordinates, IDs, marks and chemistry;
SVG/PDF/PNG and supported editable exchange use the same resulting drawing.
Pinned-center and reference drafts are editor state rather than document metadata.

## Reproduce the examples

```sh
cargo run --locked --example reference_geometry_qa -- /tmp/reference-geometry
```

The generator produces editable inputs, eight renderer cases and a combined gallery
in SVG, PNG and PDF. The quarter-turn example contains separate carbocycles to
demonstrate construction geometry; it is not a porphyrin identity reference.
Desktop evidence must separately check the controls and drag interaction.
