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
port snapping observations separately. This increment does not support live
snapping while the draft is open, role/material editing, plug families,
liquids, holds, moving parts, library destinations or editing a saved
piece definition. Standard artwork is added to the common glass silhouette;
arbitrary drawn ends are not automatically trimmed to fit.
