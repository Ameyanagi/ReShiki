# Reaction and mechanism arrows

ReShiki has nine base arrow presets: forward, equilibrium, resonance, retrosynthesis, electron pair, bent/elbow, single electron, dipole and no reaction. The palette also offers bold, dashed, hollow and unequal-equilibrium variants. These are editable drawing objects; they do not infer a chemical reaction or change atom connectivity.

Choose **Arrow** (`A`) to reuse the current preset. Hold the tool or click its lower-right triangle to choose another. Click empty space to place a fixed-length arrow pointing right, or drag to choose its length and direction. The live preview uses the same geometry and style as the committed object. Fixed Angles snaps to 15°; Option/Alt permits any angle. Arrow placement does not inherit the fixed bond length. Use **Select** to select an existing arrow. With an arrow tool active, clicking an existing arrow applies that style; repeated clicks cycle its direction, half-head side or equilibrium variant. With Select or Arrow active, drag its round endpoint handles to resize/reorient it, or drag the diamond middle handle to bend it. Curved arrows also show two square controls joined to the endpoints by direction lines: drag either square to adjust departure or arrival independently while keeping both endpoints fixed. The diamond lies on the curve. Moving a cubic endpoint carries its adjacent direction control with it. Each completed drag is one Undo step; Escape cancels the pending gesture.

The Properties inspector has a fitted preview, independent full/half/absent heads at either end, solid/hollow/angled heads, color, solid/dashed/dotted strokes, line width, head length, half-width and notch depth. Numeric and hex-color fields apply with Return. Equilibrium arrows also have a reverse-shaft length ratio and shaft separation. Cross/hash no-reaction marks and a dipole cross are available. Reverse retains the curve's shape while changing its direction; Flip bend reflects the curve about its endpoint line; Straighten removes the bend. Reset arrow style restores the selected preset's appearance.

Every new document restores **JACS / ACS** defaults: Arial 10 pt, black drawing colors, 14.4 pt bonds and 0.6 pt lines. Arrow overrides stay with the drawing. Selecting a template or storing a mixed-object template preserves arrow bends and styling.

## Optional target attachments (under review)

For **Curved / electron pair** and fishhook arrows, **Attach targets (two clicks)** chooses a source atom, visible bond or positioned lone pair, then a destination. The first click highlights without changing the drawing; the second creates one cubic arrow and one Undo step. Escape cancels the pending source. Turn the checkbox off for free clicks, use Alt-click to bypass attachment, or drag for free placement. Moving a target carries its endpoint and adjacent control; tangent editing keeps the links, while endpoint dragging detaches that end. Named **Detach start** and **Detach end** actions are available in Properties. Target deletion detaches at the last resolved position, and Undo restores the reference. See the [matched desktop comparison and retained native saves](changes/mechanism-attachments.md) for issue #92, review status and verified limits. Personal visual acceptance remains pending.

## Geometry and persistence

Native document version 21 retains arrow data from earlier versions and adds optional target references and stable lone-pair IDs in the attachment change under review. Version 20 introduced independent cubic controls. An arrow keeps its endpoints, either an optional quadratic control or two independent cubic controls, and appearance. Existing free quadratic curves keep their exact saved and rendered shape until a direction control is edited. New version-21 files require a supporting build so older builds report that an update is required rather than discarding their links. Legacy curved arrows materialize their implicit bend before reflection, copying or transformation. Translation, rotation, resizing, grouping and template placement carry the controls with the endpoints. Stroke widths and head dimensions retain their publication point sizes when the geometry is resized. Attached endpoints use final-ink clearance; shrinking into fixed-size ink can require the bounded outward correction documented in the attachment review.

The same paths draw the canvas, the placement preview, SVG, PDF and PNG. Hit testing follows the curve and heads; selection bounds, lasso and alignment include the visible arrow paths and stroke extent. Hollow full heads leave an opening in the shaft instead of drawing through the hollow center.

## CDXML exchange

Supported exchange includes straight full/half-headed arrows, head shape/dimensions/notches, color, solid/dashed strokes, resonance, dipole/no-go marks, equal-length equilibrium with half heads, and single cubic or quadratic Bézier arrows with solid heads. Cubic arrows retain both independently editable controls; quadratic arrows use their exact equivalent cubic control points. Mixed-object groups retain supported arrow children.

SVG/PDF/PNG and supported CDXML/CDX export the resolved attached curve and report that editing attachment links are omitted. Native version 21 preserves those links. This drawing metadata does not change molecular connectivity or electron counts.

The importer rejects circular/elliptical arrow arcs, multi-segment arrow splines, unknown head/stroke types and unsupported decoration. The exporter rejects dotted arrows, retrosynthesis arrows, bent/unequal equilibrium, equilibrium with full/absent heads, dipoles with a tail head, and curved arrows with hollow/angled heads or dipole/no-go decoration. Those combinations remain editable and exportable in native/SVG/PDF/PNG. Interchange checks found that external saves can remove a dipole tail head or convert an angled Bézier head to solid; those combinations therefore return an explicit export error. Head dimensions are quantized to CDXML's hundredths of a point; external applications can render strokes differently.

## Verification

The original arrow implementation checks below describe an earlier build. The [0.6 release walkthrough](release-0.6-validation.md) covers the current toolbar and rendering.

Saved interchange fixtures cover arrow handles, head shapes and supported arrow variants. The [fixture directory](../tests/fixtures/) contains the editable CDXML records used by regression tests.

Desktop checks in an isolated ReShiki QA app:

- Drew a forward arrow and bent it with the middle handle; Undo/Redo restored each complete edit.
- Changed it to a blue half-head, reversed it, flipped the bend and dragged an endpoint. The inspector preview tracked the actual curve.
- Drew an equilibrium arrow, set reverse length to 0.55 and separation to 3 pt, and saved through the native dialog. The native file retained both controls and all styling.
- Exported SVG/PDF through the GUI and visually checked the rendered PDF. Attempting CDXML with the unequal equilibrium produced an explicit unsupported-combination message.
- Restarted the standalone QA bundle, reopened the saved native drawing, and checked that the controls and colors survived. Imported the externally saved seven-arrow gallery and saved it as a version 7 native file; the bundled chemistry worker also preserved both custom arrows alongside ethanol.
- Changed the text style to 20 pt, bold and red, then used New. The toolbar returned to Arial 10 pt and black; Arrow returned to Forward with a 0.6 pt stroke.

Validation: **96 Rust tests and 26 Python tests**, plus formatting and Clippy. Regression coverage includes bend hit testing, pointer editing, bounds/lasso/alignment, transformations and duplication, hollow heads, unequal equilibrium, native persistence, Undo/Redo, new-document defaults, chemistry preservation, supported CDXML exchange, and rejected lossy combinations. Local screenshots, exported figures, saved drawings and packaging logs are under ignored `artifacts/arrow-qa-20260920/`.

## Remaining scope

Optional atom/bond/lone-pair editing attachments are under review for #92. Arrows do not enforce electron accounting, map reactants to products or generate mechanisms. Multi-segment arrow splines, circular/elliptical arcs, broad hollow arrow bodies, automatic connection routing and the unsupported CDXML combinations remain gaps. The assistant can lay out branching schemes with one shared reactant; this does not add chemical electron-flow semantics. See the [feature status](feature-status.md).
