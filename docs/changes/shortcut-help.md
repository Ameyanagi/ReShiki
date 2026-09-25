# Shortcut help and direct drawing gestures

Help now opens one editable ReShiki document of labeled examples in a separate window. Copy structures into your drawing, edit them, or save your own reference. Existing work and its Undo history stay in the original window.

[Download the single reference file](../../assets/examples/shortcut-examples.rsk) · [Complete shortcut list](../contextual-shortcuts.md) · [Merged changes awaiting release](../changes-unreleased.md)

Groups and templates are available in their current form and may be extended or revised in future releases.

## Help and examples

Press **F1**, or click **Help** at the bottom of the tools. Choose **Open shortcut examples**. Browse the nine pages using the arrows on the right. Double-click a structure, copy, switch windows, and paste. Captions stay on the reference page; the underlying atoms and bonds remain editable.

![Help opens an editable reference in a separate window](../images/shortcut-help/help.png)

![One nine-page ReShiki file with copyable structures and page controls](../images/shortcut-help/gallery.png)

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

The page checker also mistook bonds batched together by the renderer for one object crossing all nine pages. It now checks each disconnected path separately. Real bonds crossing a page boundary still produce a warning. These captures use the same reference document and 44% page-fit view. The older app opened the file from disk; the new app opened its identical bundled content, so the document title differs.

| Before: a false page-edge warning                                                                                | After: all examples fit, without the warning                                                    |
| ---------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| ![False page-edge warning for examples that fit on their pages](../images/shortcut-help/page-warning-before.png) | ![The same examples fit without a false page-edge warning](../images/shortcut-help/gallery.png) |

## All nine example pages

These are native figure exports of the single editable document. Uppercase keys mean Shift plus the letter. Each page states its target context; a key can act differently at an atom, bond, selected ring, or empty canvas. Most keys were introduced in the merged stack; this gallery documents them together.

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

Captured on Apple Silicon macOS with optimized native app builds. The earlier comparison build corresponds to `7800561` in the approved stack; after captures use this change. Native UI screenshots are resized to 1600 pixels wide. Figure pages use the document's normal bond scale and a 1400-pixel export; no structures were retouched.

Regenerate the reference and optional review PDF:

```sh
cargo run --example shortcut_examples -- assets/examples/shortcut-examples.rsk /tmp/shortcut-review
pdftoppm -scale-to 1400 -png /tmp/shortcut-review/shortcut-examples.pdf /tmp/shortcut-review/page
```

Desktop checks exercised both ring gestures, the existing modifier shortcut, adding oxygen by dragging, separate-window Help, and copying a Boc example between windows. The copied example retained nine atoms, eight bonds and its seven-atom Boc definition, with no caption.

Automated checks cover event handling, click versus drag, normal and free placement, projected-ring exclusions, undo, gallery copying and native round trips, identity-preserving display changes, isolation from unrelated complexes, and page-edge detection. Application, library and focused integration tests passed; Clippy is run with warnings denied. No new unsafe blocks or unchecked unwraps are needed.
