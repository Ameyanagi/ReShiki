You are running local UI verification for Claude on macOS. Use computer use (screenshots, mouse, keyboard).

Artifact directory (save everything here): /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b/app
Call it $ART below.

App under test: a branch build of ReShiki wrapped in its own bundle:
  /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps1b2/ReShiki1bEvidence.app
  bundle id dev.reshiki.verification.ev3861b2, name "ReShiki1bEvidence".
IMPORTANT: the user's installed "ReShiki" app (bundle in /nix/store/...) is running. Never touch, focus, click in, open, or quit it. Never look the app up by the plain name "ReShiki". If you ever find yourself in a window not owned by the PID you launched, stop interacting with it immediately.

Setup:
a. Launch it yourself, exactly:
   open -n "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps1b2/ReShiki1bEvidence.app" --env RESHIKI_DATA_DIR=/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-data --args --open "$ART/../input/caffeine.mol"
   (use the absolute path for the .mol: /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b/input/caffeine.mol)
b. Find its PID: pgrep -f 'apps1b2/ReShiki1bEvidence.app/Contents/MacOS/reshiki'  (exactly one). Save `ps -o pid,lstart,args -p <PID>` to $ART/launched-process.txt. Call it $PID.
c. Its window id: /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-wid $PID   -> $WID (re-run if the window changes).
d. Attach computer use by bundle id dev.reshiki.verification.ev3861b2 (or by the bundle path). The window should show caffeine (2D) in an "Untitled" tab.

Evidence screenshots: save each with
  screencapture -x -o -l $WID "$ART/<name>.png"
(window-only capture of your PID). Use computer-use screenshots for navigating. Move the pointer off the molecule before an evidence shot unless the step says otherwise.

Drag helper (use it for ALL drags in steps 4, 6, 8 and 9, because it can hold the mouse button while capturing):
  /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag $PID x1 y1 x2 y2 [shift] [capture=/abs/path.png]
  Coordinates are in POINTS relative to the top-left corner of the app window (window frame, including the title bar). It presses at (x1,y1), moves in 12 steps to (x2,y2), waits 0.3 s, optionally captures the window while still holding, then releases. `shift` holds Shift throughout. The app window MUST be frontmost and unobstructed at those points before you run it (activate it first), because it posts real mouse events at screen positions. Work out the point coordinates from a computer-use screenshot (mind Retina scaling: the window is about 1000 x 665 points). For a double-click use computer use normally.

Feature under test (ReShiki #386 phase 1b, 3D models):
- With the whole molecule selected (Select tool, Cmd+A), Properties -> Structure has "Show as" 2D / 3D / Both. The first switch to 3D generates a conformer (a few seconds), then atoms morph from the drawing into the 3D ball & stick model over 0.75 s.
- While the whole molecule shown as 3D is selected, a small floating "Show as" bar (2D / 3D / Both) is drawn on the canvas just above the selection box (or below if no room). It hides during any drag.
- With the molecule selected, dragging inside the 3D model frame (not within ~8 px of the frame edge) rotates the model like a trackball. Shift+drag rolls it about the view axis. Double-click inside the frame resets to the opening view. Each rotation is one Undo step.
- The selection box around a 3D model has corner handles; dragging a corner resizes the model frame (edge handles are hidden).
- Properties -> Structure has "2D layout": "Original drawing" / "From the 3D view" while the model is shown. "From the 3D view" then 2D flattens the current 3D view into the 2D drawing.

Flow (keep the window size as is; you may scroll the Properties panel):
1. Activate the window. Choose the Select tool (arrow, top-left of tools), click empty canvas, then Cmd+A. Expect caffeine selected and Properties -> Structure showing Show as 2D (active) / 3D / Both (scroll Properties if needed). Save 01-selected-2d.png.
2. Morph capture: immediately before clicking "3D", run:  echo $PID > "$ART/morph-go"   then click "3D" in the Structure section at once (a background script records ~40 frames of the window when that file appears; do not delete it). Wait until generation finishes and the model is shown. Save 02-3d-shown.png with the molecule still selected (floating bar visible).
3. Save 03-bar.png (same capture; note bar position above/below the selection and which button looks active).
4. Rotate, mid-drag: with the molecule selected, use the drag helper from a point well inside the model frame near its centre-left to ~120 pt right / ~40 pt down, with capture=$ART/04-rotate-mid-drag.png. Expect the model turned and the bar hidden in that capture.
5. After release save 05-rotate-released.png. Expect new orientation kept, bar visible again.
6. Shift-roll: drag helper with `shift`, from a point right of the model centre to ~100 pt above it (an arc-ish move), inside the frame. Save 06-shift-roll.png after. Expect the model rolled in the screen plane relative to 05.
7. Double-click inside the frame on the model. Save 07-double-click-reset.png. Expect the opening orientation (compare with 02).
8. Plain drag helper rotation again (different direction than step 4) so the view differs from the opening one; save 08-rotated-before-resize.png.
9. Resize: drag helper from the bottom-right corner handle of the selection box ~100 pt right and ~80 pt down (stay inside the canvas). Save 09-resized.png. Expect a larger frame and model; corner handles only.
10. In Properties -> Structure choose "2D layout" -> "From the 3D view". Save 10-layout-from-3d.png (Structure section visible). Then click "2D" in Show as (Structure section, or the floating bar). Save 11-flattened-2d.png. Expect a 2D drawing matching the turned 3D view (not the original caffeine layout), far bonds lighter. Note status bar text.
11. Press Cmd+Z once. Save 12-undo.png. Expect the 3D model back as in step 9/10. Note status bar text.
12. Optional: right-click the molecule, hover "Show as", save 13-context-menu.png, then press Escape.

For every evidence shot record the status bar text (bottom-left), the window title, and anything that looks wrong.

Constraints:
- Do not close or quit any other app; do not change system settings; do not edit repository files; do not save the drawing.
- At the very end quit ONLY your PID: `kill $PID` (do not use the app menu, so you cannot hit the other ReShiki). Confirm with pgrep that it is gone and that the other ReShiki is still running; record both in the report.

Write $ART/report.md: for each step, what you did (including the exact drag-helper coordinates), observed vs expected (with status bar text), screenshot filename, and a pass/fail verdict. End with anything you could not verify and why.
