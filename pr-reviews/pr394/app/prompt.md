You are running local UI verification for Claude on macOS. Use computer use (screenshots, mouse, keyboard) plus shell.

Artifact directory (save everything here): /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-final/app   (call it $ART)

App under test: a branch build of ReShiki wrapped in its own bundle:
  /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps386-final-1b/ReShiki386F1bEvidence.app
  bundle id dev.reshiki.verification.ev386final1b, name "ReShiki386F1bEvidence". Contents/MacOS/reshiki is a hard link to the branch binary.
IMPORTANT: the user's installed "ReShiki" app (PID 51217, bundle in /nix/store/...) is running, and there may be other ReShiki processes. Never touch, focus, click in, open, screenshot or quit any of them. Never look the app up by the plain name "ReShiki". If you ever find yourself in a window not owned by the PID you launched, stop interacting with it immediately.

Setup:
a. Save `pgrep -lf reshiki` to $ART/installed-process-before.txt.
b. Launch it yourself, exactly:
   open -n "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps386-final-1b/ReShiki386F1bEvidence.app" --env RESHIKI_DATA_DIR=/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-final-data --args --open "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-final/input/caffeine.mol"
c. Its PID: pgrep -f 'apps386-final-1b/ReShiki386F1bEvidence.app/Contents/MacOS/reshiki' (exactly one). Save `ps -o pid,lstart,args -p <PID>` to $ART/launched-process.txt. Call it $PID.
d. Window id: /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-wid $PID -> $WID (re-run if the window changes). Append it to launched-process.txt.
e. Attach computer use by bundle id dev.reshiki.verification.ev386final1b (or bundle path). The window shows caffeine (2D). The window is about 1000 x 665 points; keep its size.

Evidence shots: screencapture -x -o -l $WID "$ART/<name>.png" (window-only capture of your PID). Before each, move the pointer off the model and off the controls you document, unless the step says otherwise.

Drag helper (use it for ALL drags; it holds the mouse button with real CGEvents and can capture mid-drag):
  /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag $PID x1 y1 x2 y2 [shift] [capture=/abs/path.png]
  POINTS relative to the top-left of the app window frame (including title bar). Presses at (x1,y1), 12 steps to (x2,y2), waits 0.3 s, optional capture while held, releases. The window must be frontmost and unobstructed. Mind Retina scaling (screenshots are 2x).

Feature (ReShiki #386 phase 1b, 3D models):
- With the whole molecule selected, Properties -> Structure has Show as 2D / 3D / Both, and a floating "Show as" bar (2D / 3D / Both) is drawn on the canvas near the selection. It must always stay fully inside the visible canvas. It hides during any drag.
- First switch to 3D generates a conformer, then atoms morph into the 3D model over 0.75 s.
- Dragging inside the selected model's frame rotates it (trackball); a hint strip "Drag to rotate · Shift-drag to roll · Double-click to reset" shows during the drag. Shift-drag rolls. Double-click resets to the opening view. NEW: the model's frame keeps its size and place while the model turns (it must NOT resize to fit the rotated model). A size set by a corner resize is kept through later rotations.
- Properties -> Structure "2D layout": "Original drawing" / "From the 3D view" (shown while the model is shown). From the 3D view + 2D flattens the current view. Original drawing + 2D restores the drawn 2D layout, with status "Original 2D drawing restored" (or similar).
- Right-click -> "Show as" submenu.

Flow:
1. Choose the Select tool (arrow, top-left of tools), click empty canvas, Cmd+A. Expect caffeine selected, Structure Show as 2D active, floating bar visible. Save 01-selected-2d.png.
2. Morph: create $ART/morph-raw if missing. Immediately before clicking 3D run: echo $PID > "$ART/morph-go" then click "3D" in the Structure section at once (a background script records ~40 frames). Wait until the model is shown. Save 02-3d-shown.png (molecule still selected, bar visible). Measure the selection frame size in screenshot pixels and record it.
3. Rotate mid-drag: drag helper from a point inside the model frame near its centre-left, ~120 pt right / ~40 pt down, with capture=$ART/03-rotate-mid-drag.png. Expect model turned, bar hidden, hint strip shown.
4. After release, save 04-rotate-released.png. Expect orientation kept, the frame the SAME size and place as in 02 (measure and compare in pixels), bar fully inside the canvas (not clipped by status bar or panel).
5. Zoom in (status bar zoom +, or Cmd+= / Cmd+plus; record the method and the zoom %) until the selection fills the full canvas height or more, keeping the molecule selected; pan if needed so the selection is centred. Save 05-zoomed-bar-clamped.png: the bar must still be fully inside the visible canvas (clamped). Then zoom back to the previous zoom % (record it) and confirm the molecule is visible again.
6. Shift-roll: drag helper with `shift`, from right of the model centre to ~100 pt above it, inside the frame. Save 06-shift-roll.png. Expect a roll in the screen plane, frame size unchanged.
7. Double-click inside the frame on the model (computer use). Save 07-double-click-reset.png. Expect the opening orientation (compare with 02).
8. Resize: drag helper from the bottom-right corner handle of the selection box ~80 pt right and ~60 pt down (stay inside the canvas). Save 08-resized.png; record the new frame size in pixels.
9. Plain rotate with the drag helper (inside the frame, ~100 pt left / 60 pt up). Save 09-resized-then-rotated.png. Expect frame size equal to 08 (record pixels).
10. Properties -> Structure "2D layout" -> "From the 3D view". Save 10-layout-from-3d.png (Structure section visible). Click "2D" (Structure section or floating bar). Save 11-flattened-2d.png (2D drawing in the turned projection, far bonds lighter). Record status bar text.
11. Click "3D" again (model back). Save 12-back-to-3d.png. Choose "2D layout" -> "Original drawing". Save 13-layout-original.png. Click "2D". Save 14-original-restored.png: expect the ORIGINAL caffeine drawing (same layout as 01) and status like "Original 2D drawing restored". Record the exact status text.
12. Press Cmd+Z once. Save 15-undo.png. Record what came back and the status text.
13. Right-click the molecule, open "Show as" submenu (hover; click if hover does not open it). Save 16-context-menu.png (if the menu is a separate window not captured by -l, also save screencapture -x -R of the app window rect to 16b-context-menu-rect.png, cropped to the app window bounds only). Press Escape.
14. Choose "Both" (floating bar or Structure section or the context menu). Save 17-both.png: expect 2D drawing and 3D model shown together. Record status text. Click empty canvas to deselect, save 18-both-deselected.png.
15. At the end quit ONLY your PID: kill $PID. Confirm with pgrep it is gone and that 51217 (and any other pre-existing ReShiki) still runs; save outputs to $ART/process-cleanup.txt.

Do not save the drawing, do not edit repository files, do not change system settings, do not quit any other app.

Write $ART/report.md: for each step what you did (exact drag-helper coordinates), observed vs expected (status bar text, frame pixel sizes, bar position, which buttons are active), screenshot filename, pass/fail. End with anything you could not verify and why.
