# Image assistant and drawing tools validation

Checked on macOS arm64, 2026-09-23, using an optimized app bundle with an isolated settings directory.

## Bond rendering and connectivity

- The reported eight-atom, ten-bond drawing is retained in `tests/fixtures/bond-join-regression.rsk`. Its shared atoms are connected in the graph; the white gaps were rendering defects.
- Normal, bold and tapered outlines share bounded corners. Solid outlines and junction patches use one fill per color, preventing antialiasing seams at three-way and four-way junctions.
- Pixel tests cover the reported junctions at three raster scales in black and rust. Another 144 cases cover 3–8 member rings, three tilts, two configured widths, mixed styles and reversed bond directions.
- Solid and hollow wedge tips retain the configured normal width. Hashed wedge bars never taper below that width.
- The broader check found a dropped closing edge when clipping a hollow outline at a crossing. Its regression test checks both retained endpoint edges and the intentional crossing gap.
- Desktop inspection found flat caps on canvas line primitives while exports used round caps. Canvas caps now match exports, including ordinary/aromatic bonds and hash bars.
- Existing snapping, ring attachment, fragment joining, crossing, label, copy/paste and undo tests pass. Unconnected crossing bonds remain separate atoms.

## Assistant and projection controls

- Native Cmd+V attaches the ferrocene source beside the compact prompt, enables Send, and leaves the canvas unchanged. It does not start generation. Review/replacement controls remain accessible in the Review menu.
- A fresh connection selects GPT-6 Astra. A live image reconstruction and visual review produced two tilted five-member rings, two circles, Fe, and two tracked centroid contacts. It requires manual chemical review.
- Desktop checks confirmed Shift+R converts a selected five-member ring without changing its member count, four X +15° steps tilt its aromatic circle to an ellipse, and Add centroid creates a selectable anchor. Saved native data retains depth and centroid membership.
- Unit tests cover inverse tilt, native persistence, centroid movement/copy/deletion, invalid data, source-image review, stale-result protection and undo. Depth emphasis does not assign chemical stereochemistry.
- Centroid diagrams are drawing representations; molecular export is blocked rather than treating their contacts as validated multicentre chemistry.

## Updating

- Manual update checks work in the desktop dialog. The installed version matches the current published release, so Update and restart is disabled in this check.
- The signed macOS v0.6.1 disk image passed checksum, Developer ID, bundle identity, notarization and version verification, and was staged without replacing an installed app.
- Helper checks cover successful replacement and injected move failure/rollback on macOS and Linux package fixtures. App tests protect unsaved drawings and assistant drafts before handoff.
- Windows and Linux installation/restart have not been exercised on their native operating systems in this session. A real upgrade from this new updater still needs a later published version.

## Ring curves, element colors, and coordination diagrams

- Select consecutive ring atoms, open Properties → Bond appearance, and choose **Toggle inner ring curve**. Selecting the entire ring produces a circle. The stroke follows native atom positions and retained projection depth; the original bond orders are preserved. Changing a bond preset clears its curve override. Breaking the ring restores the ordinary bond depiction.
- Choose **Atoms…** beside the color controls, pick an element and a hex color, then Apply. The default scope is the selection when one exists; **Whole drawing** applies to every matching atom. Bonds, other elements, captions and font settings are retained. Applying color and adding a curve are each one undoable edit.
- Tests cover partial and closed curves, mixed-color continuity, reversible tilt, persistence, broken rings, protection of stereochemical bonds, selected/whole-document coloring, and undo/redo.
- Source-image reconstruction can now build complete schemes from explicit coordinates, including reaction arrows, captions, atom colors, real-graph abbreviations, wildcard variable labels such as E, and inner ring curves. Instructions preserve ligand geometry around the metal and keep planar source drawings in 2D. Tilt is reserved for visible perspective or an explicit request.
- A live source-image check generated a flat 73-atom scheme with matching ligand silhouettes, Cu donor positions, blue Cu, red E, four partial ring curves, and four tBu abbreviations. The chemical check still flags a nitrogen valence assignment and the overall charges are captioned; this is a visual regression fixture, not a validated chemical structure. Native chemical errors are now included in preview/review feedback.
- Computer Use confirmed whole-drawing recoloring of ten N labels in one operation, Undo restoring their colors, and the inner-curve button creating a closed ring stroke.
- Preview and review detect internal atom/label overlaps, including crowding inside a single connected complex. Manual chemical review remains required for coordinate sketches. SVG/PDF/PNG and native files preserve the new appearance; CDXML export explicitly rejects ring curves and variable labels until lossless interchange is supported.
- ChemDraw's multi-centre and variable attachment commands were checked separately; see [the comparison](chemdraw-attachment-comparison.md). A drawing centroid and an E label do not replace those chemically meaningful node types.

## Checks

`cargo test --tests --no-fail-fast`: 441 passed, 3 ignored; two additional library regressions added afterward also pass. The ignored signed macOS staging test was also run separately and passed. The `rdkit-reference` complete aromatic response replay also passes after omitting zero projection depths from legacy serialized requests. Other reference-oracle suites were not rerun locally.

`cargo clippy --all-targets -- -D warnings`, formatting, optimized compilation, website diagnostics and the website build/link checks passed. Initial desktop checks used macOS automation. The Computer Use connection was subsequently restored and used for ChemDraw inspection and the new ReShiki controls.
