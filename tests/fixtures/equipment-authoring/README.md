# Equipment authoring desktop fixture

`measured-flask.rsk` contains two editable paths in one ordinary group. The
flask mouth endpoints are world points **(-24, -100)** and **(24, -100)**.
They represent **16 mm** for the capture. The second path is a rim detail.
There are no joints, holds, motion parameters or pre-existing definitions.

Use a disposable app wrapper with a fresh `RESHIKI_DATA_DIR`. Open this file,
fit the drawing, and keep the same window, zoom, theme and panel width for
all captures. Select the flask group.

1. Capture the source group and the **Define as equipment piece…** command
   in Properties or the context menu. Opening the command must leave the
   document unsaved state unchanged.
2. Choose the command. Mark the two top mouth corners, enter **16** in the
   **mm** row, then choose **Preview**. Use **Measured flask** for **Name**.
   Capture the ordinary canvas plus the **Define equipment** panel with its
   **Accept** and **Discard** footer. The segment and two endpoint circles
   identify the measured distance.
3. Tab to **Discard** and press Enter once. Verify the two paths remain
   editable and the group remains. Repeat the same points and preview.
   Also check that Escape discards from a focused name or distance field.
4. Choose **Accept**. Capture the deselected result, then select it and show
   its normal Equipment properties. The artwork stays in the same place.
5. Undo once: both original paths and their group return. Redo once: the
   single equipment piece returns. Save a new `.rsk`, reopen it, and verify
   its Equipment properties and appearance remain.

Screenshots show the editor and rendered result. Record Undo/Redo and
save/reopen observations separately because a still image cannot prove them.
Do not capture a launch screen or unrelated recovered drawing. This fixture
exercises length calibration only; use the joint fixture below to author a port.

### Recheck the review fixes

1. Open the unmodified fixture and repeat the normal acceptance walkthrough.
   The default 0.6 pt open rim must remain 0.6 pt in the accepted piece,
   including save/reopen and exports. Existing saved pieces using 0.4 pt
   details keep that weight. No preparation or style edit is required.
2. On a disposable copy, set a visible detail to a custom red stroke, or to
   2 pt. The command must identify **Ink** or the supported width and leave
   the source unchanged. Restore the supported style before proceeding.
3. With an Assistant request already running, open a valid equipment draft.
   Let its reply finish. The equipment preview and source drawing must remain
   unchanged, and the reply must remain available. With automatic application
   enabled, Discard then permits the pending additive result to apply once.
   Repeat with Accept: the equipment and additive Assistant result have
   separate drawing Undo steps. A result replacing the original source shapes
   must wait for review after acceptance changed their revision/selection.

The model/app regressions cover these asynchronous cases without a live
service. Record any native recheck at the new exact head; earlier screenshots
remain historical evidence rather than proof of the updated entry checks.

## Fixed ground-glass joint fixture

`standard-joint-neck.rsk` is editable filled glass artwork with a compatible
neck for a standard **15/25 Socket**. Its rim edges are world points
**(-28.8, 0)** and **(28.8, 0)**. The 57.6 drawing-unit gap represents the
socket's **19.2 mm outer rim**, including its walls; the nominal ground size
is 15/25. There are no definitions or joints in the source file.

Use a fresh isolated profile and open the fixture. Select its path, then:

1. Choose **Define as equipment piece…**, then **Add joint…**. This route
   does not require entering a known length first. Set Name to **Glass
   socket adapter**, keep **Socket** and **15/25**, and click the two rim
   corners. Capture the ordinary canvas showing the generated joint,
   segment, circles and outward arrow, together with the draft panel.
2. Choose **Flip**. Capture the inward arrow and attachment error. **Add
   joint** and the outer **Accept** must be disabled. Undo the draft edit:
   the outward arrow and valid ghost return without changing the drawing.
3. Choose **Add joint**. Draft Undo returns to the pending joint; draft Redo
   adds it again. Choose **Accept**. Select the piece and inspect its normal
   Equipment properties. One drawing Undo returns the original editable
   path; Redo returns the piece. Save under a new name, reopen and inspect
   its joint, then exercise ordinary snapping with a compatible cone.
4. Reopen the original fixture and repeat through **Add joint…** with
   **Cone** and **15/25**. The first size calibrates to the cone's 15 mm
   shoulder gap. Capture the cone preview, then **Cancel joint** and
   **Discard**; the original path must remain editable with no drawing Undo.
5. For a later-size failure, first use the known-length path to calibrate the
   same rim as **19.2 mm**, then add a **15/25 Socket**. Mark this rim again
   in a second **Add joint…** operation and choose **24/29**: its different
   physical opening must be refused without rescaling the existing draft.

Use the same theme, zoom, window and panel width for comparable captures.
Record source commit and packaged-binary checksum alongside the screenshots.
The stills show the editor; record Undo/Redo, Discard, save/reopen and normal
port snapping observations separately.

### Test snapping before acceptance

1. Repeat the socket fixture through **Add joint**, then choose **Test
   snapping…**. Drag the gray **Glass stopper, 15/25** toward the authored socket.
   Capture the usual green joint feedback in matched light and dark themes.
2. Turn the test piece by 90° and drag it back to the socket: it must not
   seat sideways. Choose a second socket and oppose its mouth to the draft:
   the usual mismatch feedback must appear. Choose **Glass stopper, 29/42**:
   the existing adapter offer must appear. Repeated dragging must not add
   extra adapters or seats.
3. Undo the draft's **Add joint**, then use **Flip** on the pending joint.
   The invalid draft must lose the old trial feedback. Undo and Redo the
   Flip. Choose **Finish testing** before marking a new rim.
4. Repeat a trial with a touch drag; add a second finger to pan or pinch.
   The test piece returns to its drag start and the remaining finger must
   not resume its drag. Also check pointer exit, window deactivation and
   release outside the drawing.
5. Choose **Finish testing**, then **Test snapping…** again. Only one new
   test piece appears. **Discard** leaves the source unchanged. Repeat with
   **Accept**: the authored piece stays at its original position, with no
   test piece, trial seat or adapter. Verify one drawing Undo, Redo and
   save/reopen, then capture the deselected accepted piece.

The editor does not support authoring plug families, liquids, holds, moving
parts, library destinations or editing a saved
piece definition. Standard artwork is added to the common glass silhouette;
arbitrary drawn ends are not automatically trimmed to fit.

## Shape materials and layers

`materials-and-layers.rsk` extends the measured flask with a stand rod, a
support, a thermometer stem and a stopper. All six paths are editable and
grouped. Open the fixture in an isolated profile, select its group and choose
**Define as equipment piece…**. Mark **(-24, -100)** and **(24, -100)**,
enter **16 mm**, and choose **Preview**. Use **Stoppered flask** for **Name**.

Set these controls in **Shapes**, using the selector to reach obscured paths:

| Shape | Drawing | Layer | Material | Draw as |
| --- | --- | --- | --- | --- |
| Shape 1 | Flask | Body | Glass | Joined glass |
| Shape 2 | Rim | Body | Ink | Outline |
| Shape 3 | Stand rod | Back | Metal | Solid |
| Shape 4 | Support | Back | Metal | Solid |
| Shape 5 | Thermometer stem | Inner | Metal | Solid |
| Shape 6 | Stopper | Front | Rubber | Solid |

Changing a filled glass shape to another material selects **Solid**. Open
paths offer **Detail** and **Outline**. **Joined glass** is available for a
closed outline with **Glass** material. Clicking a visible shape selects its
path and shows the same controls; measurement and snap testing keep their
own pointer gestures.

1. Capture the source and the completed draft at matched theme, zoom, window
   and panel width. Show the selected stopper and its controls. Use the
   selector to choose the support, then move it between **Back** and **Front**:
   it crosses behind or in front of the flask. Check **Inner** on the stem.
2. Undo and Redo a material change and a layer change. The source drawing
   stays unchanged until **Accept**. Select another shape without creating a
   draft Undo step. Choose **Discard** and verify every source path returns.
3. Repeat and choose **Accept**. Save under a new name, reopen, and verify
   all six shape settings. One drawing Undo restores the source group; Redo
   restores the piece. Capture the deselected result in light and dark themes.
4. In the standard-joint fixture, add a joint and start **Test snapping…**.
   Change a shape's layer and verify its preview and snap feedback update.
   Attempt to replace the joint's only glass body with **Rubber**: validation
   must preserve the previous draft and snap trial. A successful material
   change on a separate detail must also refresh the trial. **Accept** keeps
   the styled piece at its original position and discards test objects.

The model and app regressions cover invalid open fills, material/role
combinations, attachment validation, overlapping shape selection, draft
history, transactionality, save/reopen and trial refresh. Record native
interaction results and exact build details separately from screenshots.
