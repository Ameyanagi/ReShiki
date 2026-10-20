You are capturing reference screenshots of an HTML design prototype on macOS for Claude. Use computer use (screenshots, mouse, keyboard) plus shell.

Output directory: /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2/compare/proto-shots   (call it $OUT)
Prototype file: /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2/compare/prototype-artifact-89rRsm9QDX5gwanE7kokSm.html

Setup:
a. Launch a SEPARATE Google Chrome instance with its own throwaway profile, exactly:
   open -na "Google Chrome" --args --user-data-dir=/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2-chrome --no-first-run --no-default-browser-check --new-window --window-size=1500,1400 "file:///private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2/compare/prototype-artifact-89rRsm9QDX5gwanE7kokSm.html"
   Never touch, focus or close any other Chrome window or any other app (the user's own Chrome may be running; a ReShiki app is running too - never touch it).
b. Its PID: the Chrome main process whose args contain "--user-data-dir=/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2-chrome" and that has no "--type=" argument. Save "ps -o pid,lstart,args -p <PID>" to $OUT/launched-process.txt. Call it $PID.
c. Window id: /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-wid $PID  -> $WID (re-run if needed). Dismiss any first-run Chrome dialog in THAT window only.
d. Evidence captures: screencapture -x -o -l $WID "$OUT/<name>.png" (window-only capture of your PID).

The page: an app mock with molecule chips (Caffeine / Ibuprofen / 18-Crown-6) in its title bar, a "Show as" bar with 2D / 3D / Both over a paper canvas, and an inspector on the right with "3D model" -> Style (Ball & stick, Stick, Wireframe, Space-filling) and Look (Flat · journal / Glossy · slides). Leave Hydrogens = All, Atom labels = None, Depth fade 35%, Perspective 20%, Background None. NEVER drag on the model canvas (it would rotate the view; the comparison needs the opening view). If you rotate it by accident, click "Reset view".

Flow:
1. Click "3D" in the Show as bar (or "3D model" in the inspector) so only the 3D model is shown. Wait 2 s for the morph to finish.
2. For mol in [caffeine, ibuprofen] (click the chip "Caffeine" / "Ibuprofen"; wait 1 s after; check the view is still 3D only):
     for look in [flat, glossy] (click "Flat · journal" / "Glossy · slides"):
        for style in [ballstick, stick, wire, space] (click "Ball & stick" / "Stick" / "Wireframe" / "Space-filling"):
            make sure the whole model canvas AND the inspector are visible (scroll the page to the top if needed; do not resize the window between shots), move the mouse pointer off the model canvas, wait 0.5 s, then capture $OUT/<mol>-<style>-<look>.png
   That is 16 captures. Keep the browser window size and scroll position identical across all 16.
3. Also capture $OUT/<mol>-strip-<look>.png for each mol/look after scrolling down so the "The four styles, same view" strip is visible (4 captures), then scroll back up.
4. At the end quit ONLY your Chrome instance: kill $PID. Confirm with pgrep that it is gone. Do not quit any other Chrome.

Write $OUT/report.md: what you did, any problem, the PID/WID, and for each capture which buttons appeared pressed (mol, look, style, show-as). Do not edit any files other than in $OUT.
