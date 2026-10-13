# Align and stretch a reference bond or edge

Available in Nightly builds; not in ReShiki 0.11.0.

Reference align/stretch turns a drawing so that one bond or shape edge points
exactly where you want, rotates copies around a shared center, and changes one
bond's length while the rest of the molecule stays rigid.

## Use it

### Align or rotate

1. Select both ends of a bond, or select a shape with a straight edge.
2. Open **Properties → Transform → Reference: align / stretch**.
3. Choose the bond or edge under **Reference**.
4. Under **Moves**, choose **Connected fragment** to move the whole molecule,
   or **Selected objects** to move only complete selected fragments and
   drawing objects.
5. Under **Pivot**, choose **Center of moved objects**, **Reference start**,
   **Reference midpoint** or **Pinned point**. **Pin selected center** pins the
   center of the current selection; **X** and **Y** show it in points.
6. Choose **Horizontal** or **Vertical**, or type an angle in **Direction** and
   choose **To angle**. 0° points right, 90° down, 180° left and −90° up.
7. To turn by a relative angle, type it in **Turn** and choose **Rotate**.
   **Copy + rotate** keeps the original and selects the rotated copy.

For a fourfold layout, pin one center and make copies of the original fragment
at 90°, 180° and 270°.

### Stretch one bond

1. Select the bond that links two parts of a molecule and open the same
   section.
2. Type a length in **Length** and choose **Set length**, or choose **Drag to
   stretch** and drag the moving end along the bond.
3. The line next to **Swap fixed end** names the fixed end, for example
   `Length 14.40 pt · C (start) stays fixed`. **Swap fixed end** fixes the
   other end instead.
4. After **Drag to stretch**, choose **Done** in the context row to return to
   Select.

Everything beyond the moving end moves with it and keeps its shape. Each
alignment, rotation, copy or stretch is one Undo step.

## Limits

- A partial molecule is rejected; select the whole fragment or use
  **Connected fragment**.
- Ring bonds cannot be stretched alone, because the rest of the ring would
  change. Bonds with other fixed attachments are rejected too.
- Stretching never merges nearby atoms or fuses rings. The shortest length is
  0.1 pt.
- The pinned center and unapplied settings are not saved in the file.
- Copies that share a point are not bonded to each other. Join them
  explicitly.

## Implementation notes

**Horizontal** and **Vertical** rotate the reference to the nearest matching
axis, using its actual angle. **To angle** uses a directed angle. These
operations accept arbitrary orientations; they do not round the reference to a
15° grid. A wedge's reference is its bond axis, retaining its endpoint order,
taper and stereochemistry. Alignment rotates the drawing in its plane and
leaves projection depth unchanged. An unchanged alignment records no Undo step.

**Center of moved objects** uses the stable center of all objects in the chosen
scope. The pin's X/Y fields are publication points in the drawing's coordinate
system. The pin stays fixed across edits and selection changes until it is
changed or another file is opened. The existing numeric Rotate field and
keyboard commands keep their relative rotation behavior. The ordinary rotation
handle remains free by default; Shift snaps that gesture to 15° steps.

**Drag to stretch** keeps the exact original bond direction, even at angles
such as 17.3°. The square marker is the fixed end and the round marker the
moving end. Drag the moving end or its branch; perpendicular pointer motion is
ignored. All atoms downstream of the moving end translate together, retaining
their internal lengths and angles. Dragging clamps the length at 0.1 pt, so it
cannot cross zero. Semantic attachment nodes are rejected. Use ordinary
selection movement for broader geometry changes.

Escape or loss of focus cancels a stretch gesture, and release outside the
canvas leaves the drawing unchanged. Switching tabs or replacing the drawing
also ends stretch mode; choose the reference again in the new drawing. `.rsk`
files keep the resulting coordinates, IDs, marks and chemistry; SVG/PDF/PNG and
supported editable exchange use the same resulting drawing. The pinned center
and the reference settings are editor state, not document metadata.

### Reproduce the examples

```sh
cargo run --locked --example reference_geometry_qa -- /tmp/reference-geometry
```

The generator produces editable inputs, eight renderer cases and a combined
gallery in SVG, PNG and PDF. The quarter-turn example contains separate
carbocycles to demonstrate construction geometry; it is not a porphyrin identity
reference. The [desktop review](changes/reference-geometry.md) includes real
control captures, saved files and an independent coordinate audit. It
distinguishes the checked align/stretch interactions from renderer-only
common-center examples.
