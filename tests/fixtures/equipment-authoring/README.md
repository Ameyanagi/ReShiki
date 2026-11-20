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
Do not capture a launch screen or unrelated recovered drawing. This slice
calibrates a fixed drawing only; it does not create a port from the mouth.
