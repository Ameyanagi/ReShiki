You are running local UI verification for Claude on macOS. Use computer use (screenshots, mouse, keyboard) plus shell.

Artifact directory (save everything here): /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2/app   (call it $ART)

App under test: a branch build of ReShiki wrapped in its own bundle:
  /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps386-2/ReShiki386P2Evidence.app
  bundle id dev.reshiki.verification.ev3862p2, name "ReShiki386P2Evidence". Contents/MacOS/reshiki is a symlink to the branch binary.
IMPORTANT: the user's installed "ReShiki" app (PID 51217, bundle in /nix/store/...) is running. Never touch, focus, click in, open, or quit it. Never look the app up by the plain name "ReShiki". If you ever find yourself in a window not owned by the PID you launched, stop interacting with it immediately.

Setup:
a. Launch it yourself, exactly:
   open -n "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps386-2/ReShiki386P2Evidence.app" --env RESHIKI_DATA_DIR=/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2-data --args --open "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2/input/own/caffeine-ballstick-flat.rsk"
b. Its PID: pgrep -f 'wf_228c0944-39d-2/target/debug/reshiki --open'  (exactly one; it is NOT 51217). Save "ps -o pid,lstart,args -p <PID>" to $ART/launched-process.txt. Call it $PID.
c. Window id: /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-wid $PID  -> $WID (re-run if the window changes).
d. Attach computer use by bundle id dev.reshiki.verification.ev3862p2 (or by bundle path). The window shows caffeine as a 3D ball & stick model, title "caffeine-ballstick-flat.rsk — ReShiki". The window is about 1000 x 665 points.

Evidence shots: screencapture -x -o -l $WID "$ART/<name>.png" (window-only capture of your PID). Before each, move the pointer off the model and off the controls you are documenting.

Drag helper (use it for the slider drag; it holds the mouse button with real CGEvents):
  /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag $PID x1 y1 x2 y2
  Coordinates are POINTS relative to the top-left corner of the app window frame (including title bar). The window must be frontmost and unobstructed. Work out coordinates from a computer-use screenshot, minding Retina scaling.

Feature (ReShiki #386 phase 2): with the 3D model selected, Properties shows a "3D model" section: Style buttons with pictograms (Ball & stick, Stick, Wireframe, Space-filling), Look (Flat · journal / Glossy · slides), Hydrogens All/Polar/None, Depth fade and Perspective sliders, Background None/Soft card, Reset view. Every change is one Undo step; one slider drag is one Undo step. The canvas draws the model as smooth image tiles, so zooming in must show smooth shading, no stepped rings (banding) on balls.

Flow (keep the window size; you may scroll or collapse other Properties sections so the 3D model section is visible - say so):
1. Choose the Select tool (arrow, top-left of tools), click the model once to select it (or click empty canvas then Cmd+A). Expect the "3D model" section in Properties. Save 01-section-ballstick.png (Ball & stick active, whole section visible as far as possible).
2. Click Style "Stick". Save 02-style-stick.png.
3. Click "Wireframe". Save 03-style-wireframe.png.
4. Click "Space-filling". Save 04-style-spacefilling.png.
5. Click "Ball & stick". Save 05-style-ballstick-again.png.
6. Click Look "Glossy · slides". Save 06-look-glossy.png. Then click "Flat · journal" again (no shot needed).
7. Slider: note the Depth fade value (expect 35%). With the drag helper drag the Depth fade slider knob to roughly 85-90% (one drag). Save 07-depth-fade-dragged.png (value and model visibly lighter at the back). Open the Edit menu of YOUR app window (menu bar while it is frontmost) and capture what the Undo item says, if anything: screencapture -x "$ART/07b-edit-menu-full-screen.png" is allowed for this one (full screen, because menus are separate windows) - then Escape.
8. Press Cmd+Z once. Save 08-undo-slider.png. Expect Depth fade back to 35%, Look still Flat, Style Ball & stick.
9. Press Cmd+Z once more. Save 09-undo-again.png. Expect that this undoes the previous change (Look back to... the Flat click from step 6 - so Glossy), proving the slider drag was a single step. Record what changed.
10. Press Cmd+Shift+Z (or Edit > Redo) until the state is: Ball & stick, Flat, Depth fade 35% (or just click Flat · journal). Click empty canvas to deselect.
11. Zoom to 300% (use the status bar zoom −/+ or Cmd+scroll or type it if the zoom field is editable; the status bar shows the zoom %, initially about 201%). Pan (two-finger scroll / scroll wheel) so several balls and bonds fill the view. Wait 1 s. Save 10-zoom-300-ballstick.png. Record the exact zoom % shown. Then also zoom to 400% or more and save 11-zoom-400-ballstick.png.
12. At the end quit ONLY your PID: kill $PID (not the app menu). Confirm with pgrep it is gone and that PID 51217 still runs; save both outputs to $ART/process-cleanup.txt.

Do not save the drawing, do not edit repository files, do not change system settings, do not quit any other app.

Write $ART/report.md: for each step what you did (exact drag-helper coordinates), observed vs expected (status bar text, slider values, which buttons are active), screenshot filename, pass/fail. End with anything you could not verify and why.
