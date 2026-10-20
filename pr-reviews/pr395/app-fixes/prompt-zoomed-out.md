Supplementary run. Follow /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-2-final/app-fixes/prompt.md for isolation rules, setup (a fresh launch, same command, same bundle; append to launched-process.txt as "RUN 2"), capture method, drag helper, frame measurement and cleanup (append to process-cleanup.txt), with these changes:

Context: in the first run (report.md) the drawing opened at 175% and at Perspective 100% the frame was larger than the canvas (only its right edge at x=1816 px visible, bar clamped at the canvas bottom). The fix promises the 100% frame is under 2x the flat frame, so it should fit once zoomed out.

Flow:
1. Select tool, click the model once to select it. Click the status-bar zoom minus button (about window point (1131,831)) until the zoom is about 110-115% (record each % shown; probably two clicks). Keep the molecule selected (re-click the model if needed). Save Q01-perspective-20-zoomed-out.png; record Perspective % and zoom %, measure frame and bar.
2. Drag the Perspective knob to 50% (adjust until it reads exactly 50%). Save Q02-perspective-50-zoomed-out.png; measure.
3. Drag to 100% (overshoot the track end). Save Q03-perspective-100-zoomed-out.png; measure: expect all four frame edges visible inside the canvas around the model, and the Show as bar just above or below the frame (not at the canvas bottom). Compute the ratio of the 100% frame size to the 20% frame size.
4. Rotate inside the frame with the drag helper ~120 pt right / 50 pt down. Save Q04-perspective-100-rotated-zoomed-out.png; measure (frame unchanged and still around the model).
5. Cmd+Z once (undo rotation), then bring Perspective back to 20% by dragging (adjust until exactly 20%). Save Q05-back-to-20-zoomed-out.png; measure and compare with Q01.
6. kill only your PID; confirm; append cleanup output.

Append a section "Run 2 (zoomed out)" to $ART/report.md with a table like the first one (readout, status, frame edges and size, frame pixel count, bar bounds and placement, pass/fail) and the exact drag coordinates. Do not modify the existing Run 1 text.
