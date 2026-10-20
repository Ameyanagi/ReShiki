You are running local UI verification for Claude on macOS. Use computer use (screenshots, mouse, keyboard) plus shell.

Artifact directory (save everything here): /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2-final/app-fixes   (call it $ART)

App under test: a branch build of ReShiki wrapped in its own bundle:
  /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps-appfix-2/ReShiki386Fix2Evidence.app
  bundle id dev.reshiki.verification.ev386fix2, name "ReShiki386Fix2Evidence". Contents/MacOS/reshiki is a hard link to the branch binary.
IMPORTANT: the user's installed "ReShiki" app (PID 51217, bundle in /nix/store/...) is running, and there may be other ReShiki processes. Never touch, focus, click in, open, screenshot or quit any of them. Never look the app up by the plain name "ReShiki". If you ever find yourself in a window not owned by the PID you launched, stop interacting with it immediately.

Setup:
a. Save `pgrep -lf reshiki` to $ART/installed-process-before.txt.
b. Launch it yourself, exactly:
   open -n "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps-appfix-2/ReShiki386Fix2Evidence.app" --env RESHIKI_DATA_DIR=/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2-appfix-data --args --open "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2-final/input/own/caffeine-ballstick-flat.rsk"
c. PID: pgrep -f 'apps-appfix-2/ReShiki386Fix2Evidence.app/Contents/MacOS/reshiki' (exactly one). Save `ps -o pid,lstart,args -p <PID>` to $ART/launched-process.txt. Call it $PID.
d. Window id: /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-wid $PID -> $WID. Append it and the window rect to launched-process.txt.
e. Attach computer use by bundle id dev.reshiki.verification.ev386fix2. Keep the window size as it opens (previous run: 1280 x 852 pt, caffeine opened at 158%; keep that zoom).

Evidence shots: screencapture -x -o -l $WID "$ART/<name>.png" (window-only). Move the pointer off the model and controls before each.
Drag helper (real CGEvents): /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag $PID x1 y1 x2 y2 [shift] [capture=/abs/path.png]  (POINTS relative to window frame top-left incl. title bar; screenshots are 2x).

Frame measurement: the selection frame is drawn with solid RGB (62,133,117) 2-px strokes. Measure by scanning the PNG for long straight runs of that colour inside the canvas (exclude handles); report edges, size in px, and the count of frame-colour pixels in the canvas. Also report the floating "Show as" bar's position (light rounded bar with 2D/3D/Both) relative to the frame: it should sit just above or below the frame, not at the canvas bottom.

Fix under test (ReShiki #386 phase 2): at Perspective 100% the selection frame used to vanish (it grew ~6x off the view) and the bar dropped to the canvas bottom. Now the frame must stay around the model at any perspective (under twice the flat frame at 100%).

Flow:
1. Select tool (arrow, top-left), click the model once to select it. Make the 3D model section of Properties with the Perspective slider visible (scroll the inspector if needed; previous run's slider was at about y=784 pt, knob ~1060 pt, track end ~1252 pt). Save P01-perspective-20.png (record Perspective %, frame measurement).
2. Drag the Perspective knob to 50% with the drag helper (read the value; adjust with further small drags until it reads 50%). Save P02-perspective-50.png; record value, frame, bar position.
3. Drag the knob all the way right (overshoot the track end) so it reads 100%. Save P03-perspective-100.png; record value, frame (must be visible around the whole model), bar position (above or below the frame, fully in the canvas).
4. Bring Perspective back to 20% (drag the knob, adjusting until it reads 20%; or Cmd+Z repeatedly — record which). Save P04-back-to-20.png; record value and frame; compare with P01.
5. Optional, if quick: at 100% again rotate the model once with the drag helper inside the frame (~120 pt right / 50 pt down) and save P05-perspective-100-rotated.png (frame still around the model). Then Cmd+Z as needed.
6. Quit ONLY your PID: kill $PID. Confirm with pgrep it is gone and 51217 still runs; save to $ART/process-cleanup.txt.

Do not save the drawing, do not edit repository files, do not change system settings, do not quit any other app.

Write $ART/report.md: for each step what you did (exact drag-helper coordinates), observed vs expected (slider value, status text, frame px, bar position), filename, pass/fail. End with anything you could not verify and why.
