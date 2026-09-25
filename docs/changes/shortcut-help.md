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

Native `.rsk` keeps the coordinates and editable objects. Cp/arene perspective now also transfers as editable CDX, as verified below. Complete original tilt metadata is retained in native documents; it is not guaranteed through external editors.

## Editable Cp and arene round trips

Ordinary Copy now carries the tilted Cp/arene examples as editable ring atoms and bonds, including aromatic bond order, perspective edges, multicenter targets and hidden ligand charges. It no longer forces C/H labels over the ring. This works for both individual complexes and the complete shortcut gallery.

| Before: overlapping carbon labels after paste                                                     | After: editable skeletal rings after paste                                                                   |
| ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| ![Reported gallery paste with overlapping C/H labels](../images/shortcut-help/pi-copy-before.png) | ![Gallery pasted as native editable rings with aromatic ellipses](../images/shortcut-help/pi-copy-after.png) |

The before image is the reported section 7 paste; the after image captures the same section in the receiving desktop editor. The captures use different zoom levels and crops; no chemical objects were retouched. The typed Cp* entry expands into its editable group in that editor. Metal charges stay as entered.

The cause was in the editable representation: explicit hydrogen counts forced carbon labels, and an incorrectly closed ellipse caused the receiving editor to discard aromatic bond orders. Closed ellipses now use four cubic segments with twelve control points and the native implied-boolean encoding. Hidden Cp charges use attached invisible symbols, positioned away from the metal to prevent reassignment to it. Charge text also uses the supported minus encoding so NO2/N3 group definitions survive a return copy.

The complete gallery was copied out, selected in the receiving editor, copied back and pasted into the optimized ReShiki app. The saved return document contains 483 nodes (478 atoms and five attachment points), 429 bonds and 132 captions, including 33 aromatic bonds, three hidden Cp charges and the Fe2+ charge. Native clipboard captures contain editable objects and no embedded picture. Regression tests check both directions and retain unrelated curve graphics.

For the close-up below, section 7 was selected from that saved return document and copied into a separate native drawing.

![Returned ligand examples selected as editable objects in the release app](../images/shortcut-help/pi-copy-return.png)

[Editable two-Cp source](fixtures/pi-ligands-copy.rsk) · [Captured binary fixtures and provenance](../../tests/fixtures/ligand-exchange/README.md) · [Compatibility conversions](../clipboard.md#changes-made-for-an-external-copy). R/X labels returned as query nicknames are imported as uninterpreted atom text with a visible notice; molecular properties and query semantics are unavailable for that drawing. Other unsupported query chemistry remains rejected.

## Move a ligand from its attachment point

With Select, dragging a centroid or multi-center attachment point moves its ring and covalent substituents together. The metal and other ligands stay in place unless they are also selected. The same behavior applies to keyboard nudging; stored projection depth and target membership are unchanged. Bond-length/angle constraints still apply, and Option/Alt frees the movement without separating the point from its ligand.

For a deliberate endpoint adjustment, right-click the attachment and choose **Move attachment point only**, then drag the point. Escape returns to Select. A drawing centroid is calculated from its members and cannot be independently repositioned this way.

| Before: the point leaves the ring                                                                               | After: the ligand follows its point                                                                           |
| --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| ![Dragging the point detaches its location from the arene drawing](../images/shortcut-help/centroid-before.png) | ![Dragging the point translates the arene while keeping Ru fixed](../images/shortcut-help/centroid-after.png) |

Both captures use the same [arene/Ru input](fixtures/attachment-movement.rsk) at 250% zoom and the same Option-drag offset. The title differs because the after build opens a copy. Selection changes can recenter a fitted view; saved coordinates confirm that Ru remains fixed. The ligand was already tilted in both builds; only the drag behavior differs.

## Front and back after dragging or tilting

The ligand's stored X/Y/Z coordinates now determine clearance at each crossing. Moving it across the metal can put the contact in front of the far ring edge and ellipse, or behind the near side. Previously, the contact remained behind every edge regardless of its depth. Both the outline and inner curve now follow the geometry, including contacts crossing a ring vertex.

| Before: contact incorrectly hidden by the far side                                                                | After: clearance follows depth                                                                           |
| ----------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| ![Ru contact incorrectly interrupted by the far arene edge and ellipse](../images/shortcut-help/depth-before.png) | ![Ru contact passes in front of the far arene edge and ellipse](../images/shortcut-help/depth-after.png) |

These native screenshots open the same [saved ligand drawing](fixtures/ligand-depth.rsk) at 250% zoom. The before build is `83e93e8`; the after build includes this fix. No coordinates or bond orders were changed between these captures.

Dragging translates the ligand without changing its orientation or depth. Further 3D tilts recalculate its thick and tapered perspective edges. Ordinary stereochemical wedges keep their meaning, and explicit front/back controls remain available. Automated cases cover Cp and arene, older saved shortcut contacts, crossing both sides of one ring, depth interpolation, reversible tilt, and save/reopen.

The expanded [editable depth examples](fixtures/depth-examples.rsk) cover arene movement and further tilt, Cp vertex crossings, partial inner curves, a Cp* dimer with automatic contact depth, assistant retilting, and an unchanged Haworth projection. Each case was exported as SVG, PNG and PDF. The dimer is a drawing regression: its complete coordination and formal-charge assignment is not chemically validated.

![Eight examples of ligand depth, ring curves, further tilting and retained Haworth appearance](../images/shortcut-help/depth-examples.png)

Regenerate these examples with `cargo run --example projection_depth_qa -- /tmp/projection-depth-review`.

Checking the dimer exposed a second cause of incorrect clearance: generated Cp/Cp* contacts defaulted to an explicit back layer. With two parallel ligands on opposite sides of their metals, the lower-left contact crosses a far edge and must stay in front; the upper-right contact crosses a near edge and belongs behind it. Generation now defaults to automatic depth. A deliberate front/back override remains available to reproduce a reference drawing.

| Before: both contacts forced behind                                                                                        | After: each crossing follows depth                                                                                                                          |
| -------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ![Lower-left Rh contact incorrectly interrupted at the far Cp star edge](../images/shortcut-help/cp-star-depth-before.png) | ![Lower-left Rh contact continuous across the far edge, with the upper-right contact behind its near edge](../images/shortcut-help/cp-star-depth-after.png) |

These native figure exports use the same source sketch, ring and methyl coordinates, bond orders and contact styles. Only the generated contact layers change. The test also covers omitted and null defaults, explicit front/back requests, rotated drawings, and serialized requests.

## Add bonds in a tilted ring's plane

New bonds grown from Cp and aromatic ring atoms follow the ring's retained plane. Bond-tool clicks, drags, element-tool drags and direct keyboard bond growth use the same XYZ calculation. Length and 15-degree angle increments are measured within the plane, so a foreshortened bond can appear shorter on screen. The live preview retains the same depth as the placed bond.

Option/Alt frees the angle and length while keeping the new endpoint in the plane. Existing target atoms keep their coordinates when connected. Center-to-metal contacts, ordinary chains, nonaromatic rings and nonplanar or ambiguous rings retain their existing behavior. This rule applies to the immediate bond grown from a ring atom; it does not assign a plane to an entire new side chain.

Cp substitution replaces the ring carbon's stored hydrogen and retains its chemical charge. Tests verify that adding before or after a tilt yields matching coordinates and angles, including every Cp carbon, Undo/Redo, save/reopen and edge-on views. The retained coordinates describe a drawing projection, not a calculated molecular conformer.

![New green bonds remain in the Cp and aromatic ring planes after tilting](../images/shortcut-help/plane-growth.png)

[Editable examples](fixtures/plane-growth.rsk). Regenerate them with `cargo run --example plane_growth_qa -- /tmp/plane-growth-review`. The added bonds are colored green for review.

## Bold edges meet alternating ring bonds

Automatic double-bond placement puts a ring's outer line on its skeleton. The join builder now uses that resolved placement too, so a neighboring bold bond shares its corners with the thin outer line. Previously, the join builder excluded automatic plain double bonds, leaving square protrusions at the ends of the bold edge.

| Before: protruding square caps                                                                                     | After: shared ring corners                                                                                  |
| ------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| ![A bold arene edge ends in square protrusions beside double bonds](../images/shortcut-help/arene-bold-before.png) | ![The bold arene edge joins the neighboring thin outer lines](../images/shortcut-help/arene-bold-after.png) |

Both figures use the same [saved arene drawing](fixtures/arene-bold-join.rsk). Only rendering changes; the coordinates, double bonds and green C–F bond are unchanged. Tests cover shared corners, rotated/reversed bonds, several tilt angles, transparent raster coverage and preservation of the separate inner double-bond lines.

The green C–F branch exposed a separate issue: joining excluded adjacent bonds with different colors. Widths and directions now determine the junction regardless of color. At an unambiguous thick/thin ring corner, the ring keeps its own outline and the outgoing substituent meets its exterior. The branch no longer changes the corner into a pointed shoulder or paints a colored triangle inside the ring. Tests compare the ring silhouette with and without its substituent across rotations and tilts.

| Before: colored branch excluded from the join                                                          | After: all three bonds share the junction                                                                         |
| ------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------- |
| ![Green C–F bond meets a protruding black ring corner](../images/shortcut-help/arene-color-before.png) | ![Green C–F bond joins the black ring without a protruding corner](../images/shortcut-help/arene-color-after.png) |

## Copying the bold arene

The pictured arene can now be copied as editable CDX/CDXML, including its bold edge, green bond and original alternating-bond placement. The exporter permits bold emphasis on chemically aromatic edges without assigning tetrahedral stereochemistry. Tests check the molecular identifier, atom and bond counts, colors and bond orders after both formats are reimported. Native clipboard data retains the XYZ coordinates and projection flags.

The optimized macOS build was also tested through the actual system clipboard: copy the selected arene, paste into an external drawing editor, choose **As Copied** if prompted about drawing settings, then copy it back. Both captured CDX files contain seven atoms and seven bonds, with no embedded picture. The returned structure retains three double bonds, the bold single edge and green C–F bond. The captured files are regression fixtures.

![The release build reports editable drawing and images copied for the selected arene](../images/shortcut-help/editable-arene-release.png)

Cp/arene perspective wedges and hidden ligand charges now transfer as editable CDX as well. Other unsupported features are simplified only in the external copy, with a notice; see the [compatibility table](../clipboard.md#changes-made-for-an-external-copy). When a picture fallback is still necessary, it says **Copied · editable in ReShiki; picture in other apps**, with details available on hover. A successful copy no longer displays a multiline red error. External editable transfer retains the tested 2D appearance; use native `.rsk` to preserve the full tilt metadata.

## Automatic formula text

Choose **Text**, click empty canvas and type a standalone neutral formula such as `C2H2`, `C2H5OH` or `Ca(OH)2`. The numbers automatically appear as subscripts in the Appearance preview and the applied caption. Ordinary captions such as `Figure 2` remain unchanged. Finish with **Done** or **Cmd/Ctrl+Enter**.

| Before: baseline digits                                                                 | After: formula subscripts                                                                                        |
| --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| ![Formula captions with unformatted digits](../images/shortcut-help/formula-before.png) | ![Formula captions with subscripts and an unchanged Figure 2 caption](../images/shortcut-help/formula-after.png) |

[Editable examples](fixtures/formula-text.rsk). The underlying text stays editable ASCII. Native documents and figure exports retain its formatting; the caption does not add atoms or change molecular properties. Detection is deliberately limited to complete neutral formulas: prose, units, unknown symbols and ambiguous charge notation remain unchanged. Use the Style row's **CH₂** and subscript/superscript controls for manual formatting. Existing captions keep their chosen format, and a manual formula or script choice overrides detection for that editing session. Undo/Redo retains both text and formatting.

Desktop verification typed `C2H2` into a new caption, observed automatic CH₂ activation and the subscripted Appearance preview, then applied it with Cmd+Enter. The applied caption displayed C₂H₂; the molecular formula of the separate arene remained C6H5F.

![Formula captions and the corrected arene in the optimized application](../images/shortcut-help/formula-release.png)

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
