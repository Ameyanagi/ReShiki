# Shortcut help and direct drawing gestures

Help now opens one editable ReShiki document of labeled examples in a separate window. Copy structures into your drawing, edit them, or save your own reference. Existing work and its Undo history stay in the original window.

[Download the single reference file](../../assets/examples/shortcut-examples.rsk) · [Complete shortcut list](../contextual-shortcuts.md) · [Merged changes awaiting release](../changes-unreleased.md)

Groups and templates are available in their current form and may be extended or revised in future releases.

## Help and examples

Press **F1**, or click **Help** at the bottom of the tools. Choose **Open shortcut examples**. The reference is arranged in nine sections on the ordinary unbounded canvas. Scroll vertically or sideways to pan, hold Cmd/Ctrl while scrolling to zoom at the pointer, or use Fit for an overview. Double-click a structure, copy, switch windows, and paste. Captions stay in the reference; the underlying atoms and bonds remain editable.

![Help opens an editable reference in a separate window](../images/shortcut-help/help.png)

![One ReShiki file of copyable structures on the normal unbounded canvas](../images/shortcut-help/gallery.png)

## Canvas fills the drawing area

Grid, rulers and crosshair are all off by default. Toggle them under **View** beside the zoom controls.

The permanent 18-pixel gray inset, paper border and shadow are removed. With rulers disabled, drawing extends to the tool strip, inspector and status-bar edges. Enabling rulers reserves only their top and left gutters; the crosshair is drawn over the canvas. Optional page setup remains available for printing and multipage PDF export.

Normal wheel or trackpad scrolling pans in both directions. Cmd/Ctrl-scroll zooms around the pointer, including when rulers are visible. Navigation changes the camera only; it does not move the drawing objects.

| Before: permanent gray frame                                                                | After: full drawing area                                                                           |
| ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| ![The drawing canvas had a permanent gray inset](../images/shortcut-help/canvas-before.png) | ![The unbounded canvas fills the available drawing area](../images/shortcut-help/canvas-after.png) |

![Optional rulers reserve only their own gutters, with a crosshair over the canvas](../images/shortcut-help/canvas-rulers.png)

## Editable clipboard transfer

An aromatic five-membered drawing previously failed to paste when its chemical bond assignment could not be validated. It now transfers as an editable drawing, preserving the supplied atom and bond data. The warning remains visible, and molecular properties are withheld until the assignment is resolved; no charge is invented.

| Before: the drawing is rejected                                                                                 | After: the drawing stays editable                                                                 |
| --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| ![A chemical validation error prevents the drawing from being pasted](../images/shortcut-help/paste-before.png) | ![The same drawing pastes with a chemical-review notice](../images/shortcut-help/paste-after.png) |

The seven-atom, six-bond sample was copied from an external editor into ReShiki, copied back, and returned through the native clipboard. Both captured CDX inputs are regression fixtures. Automated checks also cover established bonds, labels, groups, attachment targets, graphics and fourteen Haworth drawings.

If a drawing uses appearance settings that editable CDX cannot yet preserve, Copy now includes a sized picture for other drawing editors and explains the fallback. ReShiki-to-ReShiki copying retains the editable original. This preserves appearance but does not turn unsupported objects into editable chemical structures in another app.

## Perspective ligand shortcuts

At an atom, **j** adds Cp and **J** adds an arene ligand with an initial 60° tilt, tapered front edges and a contact behind the ring. The regular ring is built in 3D before projection: its true bond lengths, aromatic order, charges and multi-center targets survive later tilts. The ellipse follows the same ring plane. Wedges here are drawing perspective, not assigned stereochemistry. At a metal, repeating the shortcut adds another ligand and keeps that metal selected. The Cp minus sign is hidden by default, while its −1 charge remains in the chemical data. Metal charges are left as entered; neutral overall charge is not assumed.

| Before: flat rings                                                                 | After: retained 3D geometry                                                                                |
| ---------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| ![Flat Cp and arene shortcut examples](../images/shortcut-help/ligands-before.png) | ![Tilted Cp and arene examples with front edges and rear contacts](../images/shortcut-help/07-ligands.png) |

Native `.rsk` keeps the coordinates and editable objects. Projected ligand styles currently use the explicit picture fallback when copied to external drawing editors; external editable projection interchange remains a limitation.

## Move a ligand from its attachment point

With Select, dragging a centroid or multi-center attachment point moves its ring and covalent substituents together. The metal and other ligands stay in place unless they are also selected. The same behavior applies to keyboard nudging; stored projection depth and target membership are unchanged. Bond-length/angle constraints still apply, and Option/Alt frees the movement without separating the point from its ligand.

For a deliberate endpoint adjustment, right-click the attachment and choose **Move attachment point only**, then drag the point. Escape returns to Select. A drawing centroid is calculated from its members and cannot be independently repositioned this way.

| Before: the point leaves the ring                                                                               | After: the ligand follows its point                                                                           |
| --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| ![Dragging the point detaches its location from the arene drawing](../images/shortcut-help/centroid-before.png) | ![Dragging the point translates the arene while keeping Ru fixed](../images/shortcut-help/centroid-after.png) |

Both captures use the same [arene/Ru input](fixtures/attachment-movement.rsk) at 250% zoom and the same Option-drag offset. The title differs because the after build opens a copy. Selection changes can recenter a fitted view; saved coordinates confirm that Ru remains fixed. The ligand was already tilted in both builds; only the drag behavior differs.

## Both ring interactions

- **During placement:** choose Benzene and hold **Cmd on Mac / Ctrl on Windows or Linux** while clicking or dragging for a circle. A normal click gives alternating bonds. The modifier also works with regular rings and cyclopentadiene; chair and Haworth tools retain their existing behavior.
- **After placement:** with Select, click inside an aromatic ring, then press lowercase **a** to switch between circle and alternating bonds. **Cmd/Ctrl+Alt+K** remains available. The change preserves molecular identity. At an individual atom or bond, **a** keeps its existing phenyl attachment / benzene fusion action.

In this native app capture, the upper middle ring was placed with a normal click, the lower middle ring with Cmd-click, and the left ring was switched with **a**.

![Alternating and circular rings placed with the same Benzene tool, plus a selected-ring display change](../images/shortcut-help/placement-gestures.png)

## Add an element and a bond together

Choose an element in **Atoms**, then **drag from an existing atom**. This adds the chosen element with a single bond at the configured length and angle. **Option/Alt** permits free placement. A click still replaces an atom; dragging onto an existing atom connects without changing its element or an existing bond's order.

The capture shows oxygen added to a two-carbon chain in one drag. The saved drawing contains three atoms, two bonds, a 42-unit new bond and a 30-degree direction.

![Dragging from carbon with oxygen selected adds a terminal oxygen and a bond](../images/shortcut-help/atom-drag.png)

## Fixes found while checking the gallery

An unrelated multi-center complex previously blocked a selected benzene's display change. The operation now checks the selected molecular components and leaves the other drawing content intact. Both captures use the same [input file](fixtures/shortcut-ring-display.rsk), select the left ring, and press **Cmd+Option+K** at 170% zoom.

| Before: the whole-document check blocks the edit                                                                         | After: the selected ring changes; the complex stays intact                                                                        |
| ------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------- |
| ![Unrelated attachment error blocks the selected benzene display change](../images/shortcut-help/ring-toggle-before.png) | ![The selected benzene changes to a circle without changing the unrelated complex](../images/shortcut-help/ring-toggle-after.png) |

For optional printing, the page checker also mistook bonds batched together by the renderer for one object crossing multiple print pages. It now checks each disconnected path separately. Real bonds crossing a page boundary still produce a warning. These historical captures use the [same print-layout fixture](https://github.com/Ameyanagi/ReShiki/blob/bee1669432280851cf677556435a0183c5ad9b21/assets/examples/shortcut-examples.rsk) at 44%. The current gallery opens without a print layout. The older capture opened the fixture from disk, so the titles differ.

| Before: a false page-edge warning                                                                                | After: all examples fit, without the warning                                                               |
| ---------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| ![False page-edge warning for examples that fit on their pages](../images/shortcut-help/page-warning-before.png) | ![The same examples fit without a false page-edge warning](../images/shortcut-help/page-warning-after.png) |

## All nine example sections

These are native figure exports of the nine sections in the single editable document. Uppercase keys mean Shift plus the letter. Each section states its target context; a key can act differently at an atom, bond, selected ring, or empty canvas. Most keys were introduced in the merged stack; this gallery documents them together.

### 1. Common groups

![Common groups: Me, Et, Boc, Ac, CO2Me, CF3, Cbz, NO2, OMe, Ph, Fmoc and N3](../images/shortcut-help/01-groups.png)

### 2. Elements, variables and charges

![Element, isotope, variable-label and formal-charge shortcut examples](../images/shortcut-help/02-atoms.png)

### 3. Atom growth

![Branching, chain extension, carbonyl, wedge, alkene, alkyne and group growth](../images/shortcut-help/03-growth.png)

### 4. Ring attachment and display

![Attached ring sizes, phenyl attachment and selected aromatic circle display](../images/shortcut-help/04-attached-rings.png)

### 5. Bond styles and double-line placement

![Single, double, triple, dashed, bold, wedge, hashed and wavy bonds with double-line placement](../images/shortcut-help/05-bonds.png)

### 6. Ring fusion

![Fused rings, aromatic and diene fusion and chair orientations](../images/shortcut-help/06-fused-rings.png)

### 7. Groups, ligands and typed labels

![MgBr, Cp, arene and repeated ligand insertion, plus typed C2H5, Cp star, Boc and NH3](../images/shortcut-help/07-ligands.png)

### 8. Drawing tools and placement gestures

![Bond tools, chain, benzene, cyclopentadiene, reaction arrow, Cmd/Ctrl ring placement and element drag](../images/shortcut-help/08-tools.png)

### 9. Selection transforms

![Rotation, tilt and flip shortcuts](../images/shortcut-help/09-transforms.png)

## Reproduction and checks

Captured on Apple Silicon macOS with optimized native app builds. The ring and print-warning comparisons use `7800561` as the earlier build. The canvas-inset comparison uses `bee1669` as its earlier build; after captures use this change. Native UI screenshots are resized to 1600 pixels wide. The canvas comparison opens the same empty drawing with rulers off at 250% zoom. Figure pages use the document's normal bond scale and a 1400-pixel export; no structures were retouched.

Regenerate the reference and optional review PDF:

```sh
cargo run --example shortcut_examples -- assets/examples/shortcut-examples.rsk /tmp/shortcut-review
pdftoppm -scale-to 1400 -png /tmp/shortcut-review/shortcut-examples.pdf /tmp/shortcut-review/page
```

Desktop checks also verified ligand dragging, Undo and explicit point-only movement. Saved coordinates confirm that the ring and its point translate together, Ru stays fixed, and every depth value is retained. Desktop checks exercised both ring gestures, the existing modifier shortcut, adding oxygen by dragging, separate-window Help on an unbounded canvas, optional rulers/crosshair, and copying a Boc example between windows. The copied example retained nine atoms, eight bonds and its seven-atom Boc definition, with no caption.

Automated checks cover event handling, click versus drag, normal and free placement, projected-ring exclusions, undo, gallery copying and native round trips, identity-preserving display changes, isolation from unrelated complexes, and page-edge detection. Application, library and focused integration tests passed; Clippy is run with warnings denied. No new unsafe blocks or unchecked unwraps are needed.
