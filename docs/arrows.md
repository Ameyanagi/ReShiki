# Reaction and mechanism arrows

ReShiki now has eight arrow presets: forward, equilibrium, resonance, retrosynthesis, electron pair, single electron, dipole and no reaction. These are editable drawing objects; they do not infer a chemical reaction or change atom connectivity.

Choose **Arrow** (`A`), pick a preset, and drag. The live preview uses the same geometry and style as the committed object. Fixed Angles snaps to 15°; Option/Alt permits any angle. Arrow placement does not inherit the fixed bond length. Click an existing arrow to select it. With Select or Arrow active, drag its round endpoint handles to resize/reorient it, or drag the square middle handle to bend it. The square lies on the curve. Each completed drag is one Undo step; Escape cancels the pending gesture.

The Properties inspector has a fitted preview, independent full/half/absent heads at either end, solid/hollow/angled heads, color, solid/dashed/dotted strokes, line width, head length, half-width and notch depth. Numeric and hex-color fields apply with Return. Equilibrium arrows also have a reverse-shaft length ratio and shaft separation. Cross/hash no-reaction marks and a dipole cross are available. Reverse retains the curve's shape while changing its direction; Flip bend reflects the curve about its endpoint line; Straighten removes the bend. Reset arrow style restores the selected preset's appearance.

Every new document restores **JACS / ACS** defaults: Arial 10 pt, black drawing colors, 14.4 pt bonds and 0.6 pt lines. Arrow overrides stay with the drawing. Selecting a template or storing a mixed-object template preserves arrow bends and styling.

## Geometry and persistence

The native document version is now 7; versions 1–6 remain readable. An arrow keeps its endpoints, optional quadratic control point and appearance. Legacy curved arrows materialize their implicit bend before reflection, copying or transformation. Translation, rotation, resizing, grouping and template placement carry the control point with the endpoints. Stroke widths and head dimensions retain their publication point sizes when the geometry is resized.

The same paths draw the canvas, the placement preview, SVG, PDF and PNG. Hit testing follows the curve and heads; selection bounds, lasso and alignment include the visible arrow paths and stroke extent. Hollow full heads leave an opening in the shaft instead of drawing through the hollow center.

## CDXML exchange

Supported exchange includes straight full/half-headed arrows, head shape/dimensions/notches, color, solid/dashed strokes, resonance, dipole/no-go marks, equal-length equilibrium with half heads, and single quadratic Bézier arrows with solid heads. Bézier arrows use the exact equivalent cubic control points. Mixed-object groups retain supported arrow children.

The importer rejects circular/elliptical arrow arcs, multi-segment or non-quadratic splines, unknown head/stroke types and unsupported decoration. The exporter rejects dotted arrows, retrosynthesis arrows, bent/unequal equilibrium, equilibrium with full/absent heads, dipoles with a tail head, and curved arrows with hollow/angled heads or dipole/no-go decoration. Those combinations remain editable and exportable in native/SVG/PDF/PNG. Interchange checks found that external saves can remove a dipole tail head or convert an angled Bézier head to solid; those combinations therefore return an explicit export error. Head dimensions are quantized to CDXML's hundredths of a point; external applications can render strokes differently.

## Verification

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

Arrows do not attach semantically to atoms/bonds, move with a connected atom, enforce electron accounting, map reactants to products or generate mechanisms. Arbitrary splines, circular/elliptical arcs, broad hollow arrow bodies, connection/branching tools, richer reaction schemes and the unsupported CDXML combinations remain gaps. See the [feature status](feature-status.md).
