# ReShiki #386 phase 1b — local UI verification

Run on 20 October 2026 (Asia/Tokyo), using the isolated `ReShiki386Fix1bEvidence.app` build and the supplied caffeine input. **Both targeted fixes passed the observed UI checks.** The resized/rotated 3D frame returned with **0 px movement on every edge** after the flattened-2D round trip. Restoring **Original drawing** produced the original caffeine drawing in 2D and in Both, including after Undo/Redo and after redoing the layout restoration. Two requested details have limitations: this build exposes no native Edit menu, and choosing Both replaces the earlier redo branch for switching to 2D. Those limitations and the additional restore-redo check are documented below.

**Process isolation and capture conditions**

- Saved the pre-launch `pgrep -lf reshiki` output in [installed-process-before.txt](installed-process-before.txt). Only the pre-existing installed PID `51217` was present.
- Executed the exact `open -n …ReShiki386Fix1bEvidence.app --env RESHIKI_DATA_DIR=…/ev386-1b-appfix-data --args --open …/input/caffeine.mol` launch command from `prompt.md`.
- Exact bundle-path `pgrep` returned one PID: **76822**. Saved its start time and arguments in [launched-process.txt](launched-process.txt).
- The supplied window helper returned **WID 7636**. CoreGraphics confirmed owner PID 76822, origin **(200,115)** screen points, size **1280 × 852** points. The window size was kept unchanged. Every formal screenshot is **2560 × 1704** pixels, captured only with `screencapture -x -o -l 7636 <filename>`.
- Computer use attached only to bundle ID `dev.reshiki.verification.ev386fix1b`. No installed ReShiki app was looked up by name, focused, clicked, captured, or quit.
- The computer-use tool sometimes returned foreground focus to Safari. For supplied-helper drags and captures, [tools/verify-window.swift](tools/verify-window.swift) verified the exact PID/window pair, activated only PID 76822 after checking its bundle ID, and verified foreground ownership. Pointer parking was at window-relative **(960,60) pt**, a blank header area away from model and documented controls. Checks are in [window-checks.txt](window-checks.txt). Early foreground guards stopped before pointer movement; A01 was recaptured after the guard succeeded. From A09 onward, parking also allowed 0.2 seconds for hover repaint.
- [A00-opening-2d.png](A00-opening-2d.png) is an extra initial reference at **238%**, before the formal flow and before pointer parking was established. It shows the imported, upright drawn caffeine layout. Formal evidence is A01–A16.
- No drawing was saved, no repository files were edited, and no system settings were changed. Generated tools, measurements, and reports are confined to this artifact directory.

**Coordinate convention and exact actions**

Computer-use coordinate clicks below are **2× screenshot pixels relative to the window top-left**; the corresponding point coordinates are also given. The supplied drag helper uses **window-relative points including the title bar**. AX actions are identified by their exact accessibility ID; an AX action does not supply a caller-selected x/y, so no fabricated click coordinate is assigned to those two initial actions.

| Control/action | Exact computer-use input | Window-relative point equivalent |
|---|---|---|
| Initial arrow selection | AX `tool-Select`, then-current index 80 | AX action; no caller-specified coordinate |
| Initial 3D selection | AX `structure-show-3D`, then-current index 122 | AX action; no caller-specified coordinate |
| Arrow recheck before exiting keyboard mode | `(52,374)` px | `(26,187)` pt |
| Empty canvas | `(400,400)` px | `(200,200)` pt |
| Zoom minus, each click | `(2263,1663)` px | `(1131.5,831.5)` pt |
| From the 3D view | `(2380,746)` px | `(1190,373)` pt |
| Properties → Structure → 2D | `(2048,548)` px | `(1024,274)` pt |
| Properties → Structure → 3D, return | `(2260,548)` px | `(1130,274)` pt |
| Original drawing | `(2130,731)` px | `(1065,365.5)` pt |
| Properties → Structure → Both | `(2394,548)` px | `(1197,274)` pt |
| App menu inspection | AX menu-bar item `ReShiki386Fix1bEvidence`, then-current index 69 | AX action; no caller-specified coordinate |

A stale initial AX index 16 was rejected without executing an action. The tree was refreshed before using index 80. Keyboard commands were sent only through the test-app binding: `super+a`, `F8`, `super+z`, `super+shift+z`, and `Escape`.

All three drag attempts used exactly the supplied executable:

```text
/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag 76822 704 674 784 734
/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag 76822 704 674 784 734
/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag 76822 580 530 480 470
```

No Shift modifier was used. [drag-actions.txt](drag-actions.txt) records each helper completion against WID 7636 and origin (200,115).

**Step-by-step observations**

| Prompt step / evidence | Action and expected result | Observed state, status, and measurement | Result |
|---|---|---|---|
| 1 — [A01-3d-122.png](A01-3d-122.png) | Arrow; empty canvas `(400,400)` px; Cmd+A; initial 3D AX action; let the model settle; zoom minus `(2263,1663)` px three times. Expected 122%, selected 3D, 744 × 744 px frame. | Zoom **122%**, 3D active in Properties and floating Show as; Original drawing active. Status **“Shown as 3D model · Undo restores the 2D drawing”**. Frame L664..665 / T603..604 / R1408..1409 / B1347..1348; **744 × 744 px**, center **(1036.5,975.5)**. | **PASS** |
| 2 — [A02-resize-first-attempt.png](A02-resize-first-attempt.png) | Verified bottom-right handle near `(704.5,674)` pt from A01; attempted supplied-helper `(704,674) → (784,734)` pt. | Frame stayed **744 × 744 px**. Keyboard drawing was still active from opening, and the gesture moved the keyboard drawing cursor instead of resizing. Status remained the initial 3D message. | **SETUP ATTEMPT FAILED**, retained as evidence |
| 2 — [A02-resized.png](A02-resized.png) | Rechecked arrow at `(52,374)` px, then **F8** to leave keyboard drawing; repeated exactly `(704,674) → (784,734)` pt. Expected 884 × 884 with top-left fixed. | Frame L664..665 / T603..604 / R1548..1549 / B1487..1488; **884 × 884 px**, center **(1106.5,1045.5)**. Top-left unchanged; right and bottom each +140 px. 3D and Original drawing active, 122%. Status **“Drawing updated”**. | **PASS after documented setup adjustment** |
| 3 — [A03-resized-rotated.png](A03-resized-rotated.png) | Supplied-helper `(580,530) → (480,470)` pt, inside frame. Expected model rotation with frame fixed. | Model visibly tilted/rotated. Frame remains **884 × 884 px**, same four edges and center as A02. 3D / Original drawing / 122%. Status **“Drawing updated”**. This is the BEFORE frame. | **PASS** |
| 4a — [A04-layout-from-3d.png](A04-layout-from-3d.png) | Click From the 3D view `(2380,746)` px. | From the 3D view active; Original drawing off; 3D stays active. Frame unchanged at **884 × 884 px**. Status remains **“Drawing updated”**. Explanatory text says to flatten the model as currently turned, with far bonds lighter. | **PASS** |
| 4b — [A05-flattened-2d.png](A05-flattened-2d.png) | Click 2D `(2048,548)` px. Expected projection of the rotated model. | 2D active in Properties and Show as. Canvas shows the slanted, compressed projection, visibly distinct from the original upright drawing. Status **“Shown as 2D from the 3D view · Undo restores the 3D model”**. 122%. The 2D layout row is not displayed in pure 2D mode. Compact selection box is visible, but its exact-color lines do not meet the >400-pixel measurement threshold. | **PASS** |
| 5 — [A06-back-to-3d.png](A06-back-to-3d.png) | Click 3D `(2260,548)` px. Expected identical frame to A03. | Rotated 3D model returns; 3D active, From the 3D view active, 122%. Status **“Shown as 3D model · Undo restores the 2D drawing”**. L664..665 / T603..604 / R1548..1549 / B1487..1488; **884 × 884 px**, center **(1106.5,1045.5)**. Each edge differs from A03 by **0 px**. | **PASS — fix 1** |
| 6 — [A07-layout-original.png](A07-layout-original.png) | Click Original drawing `(2130,731)` px. Expected original coordinates restored while model remains 3D. | Original drawing active, From the 3D view off; Properties and Show as remain 3D. Canvas still shows the rotated 3D model. Status **“Original 2D drawing restored”**. Frame exactly matches A03/A06: **884 × 884 px**, zero edge or center movement. Hidden 2D restoration is confirmed by A08 and subsequent Both views. | **PASS** |
| 7 — [A08-original-2d.png](A08-original-2d.png) | Click 2D `(2048,548)` px. Expected original caffeine layout. | Upright fused 6+5 ring drawing matches A00: left carbonyl points left, lower carbonyl down, five-membered ring on the right. It differs clearly from A05. 2D active; layout row hidden; 122%. Status **“Shown as 2D · Undo restores the 3D model”**. Measured 2D selection: **545 × 463 px** by corresponding-edge formula; see stroke-thickness caveat in measurements. | **PASS** |
| 8 — [A09-undo.png](A09-undo.png) | Cmd+Z once; inspect native menu / Redo availability; close menu with Escape before screenshot. | Returns to rotated **3D**, with **Original drawing** still active. Floating Show as also 3D. Status **“History restored”**. Frame restored at **884 × 884 px** and original center. Toolbar AX `header-redo` is enabled (not marked disabled); screenshot also shows the enabled arrow. Native app menu inspection exposes About/Services/Hide/Quit only; no Edit menu exists in the exposed menu bar. Escape was sent before A09; stale menu AX state was later dismissed with its Cancel action and another Escape. | **PASS for Undo/layout/Redo availability; Edit-menu-specific check unavailable** |
| 9a — [A10-both-122.png](A10-both-122.png) | Click Both `(2394,548)` px. | Both active in Properties and Show as; Original drawing active. Original upright 2D member is at left, rotated 3D member at right. Status **“Shown as 2D and 3D · Undo restores the 3D model”**. 122%. Right portion of the 3D member and the selected union frame extend beyond the canvas under the inspector. | **PASS for restored 2D layout** |
| 9b — [A11-both-97.png](A11-both-97.png) | Zoom minus `(2263,1663)` px once. | Zoom **97%**. Both and Original drawing remain active. Both molecules are visible; 2D is the original drawn caffeine, not A05’s projection. Same “Shown as 2D and 3D…” status. The union frame’s right edge is still clipped; no full frame width is inferred. | **PASS — fix 2** |
| 9c — [A12-both-deselected.png](A12-both-deselected.png) | Empty canvas `(400,400)` px; capture, then Cmd+A again. | Both molecules remain visible at 97%; clean unselected 2D member clearly has the original layout. Selection frame and Show as disappear; inspector says to select an object, so no active layout row is displayed while deselected. Status stays **“Shown as 2D and 3D · Undo restores the 3D model”**. | **PASS** |
| 10a — [A13-undo-both.png](A13-undo-both.png) | After Cmd+A, Cmd+Z once. Expected undo Both. | 3D only; 3D active, Original drawing active, 97%; Redo enabled. Status **“History restored”**. Exact-color corresponding-edge frame size **717 × 717 px**, center **(1102,1034.75)**; bottom stroke has only one exact-color row at this zoom, so raw details are retained. | **PASS** |
| 10b — [A14-redo-both.png](A14-redo-both.png) | Cmd+Shift+Z once. Expected Both with original drawing. | Both returns; Both and Original drawing active; 97%. Left 2D member still matches A08/A12. Status **“History restored”**. Redo now disabled. Selected union frame clipped at right. | **PASS** |
| 11a — [A15a-undo-to-3d-original.png](A15a-undo-to-3d-original.png) | Cmd+Z once from A14, until 3D + Original drawing as A09. | Reached target after **one Undo**: 3D active, Original drawing active, 97%, Redo enabled, status **“History restored”**. Next Redo is the Both action, as already demonstrated in A13→A14; the older switch-to-2D redo branch was replaced when Both was chosen. | **REQUESTED 2D DISPLAY REDO BRANCH UNAVAILABLE** |
| 11b — [A15b-undo-original-restore.png](A15b-undo-original-restore.png) | Additional focused check: Cmd+Z once more to undo Original drawing restoration itself. | Model remains 3D; layout changes back to **From the 3D view**. Status **“History restored”**; 97%; frame **717 × 717 px**, unchanged from A15a. This establishes the restoration action that can be redone. | **PASS for reaching restore history entry** |
| 11c — [A15-redo-2d.png](A15-redo-2d.png) | Cmd+Shift+Z once to redo that restoration. | **Original drawing** becomes active again while **3D stays active**. Status **“History restored”**; 97%; frame unchanged at **717 × 717 px**. The required filename is retained, but this shot documents redo of the original-2D-coordinate restoration, not a switch to 2D display. | **PASS for layout-restoration Redo** |
| 11d — [A16-both-after-redo.png](A16-both-after-redo.png) | Choose Both `(2394,548)` px after restoration Redo. | Both active; Original drawing active; 97%. Left member remains the original drawn caffeine; right member is the rotated 3D model. Status **“Shown as 2D and 3D · Undo restores the 3D model”**. | **PASS** |
| 12 — [process-cleanup.txt](process-cleanup.txt) | Verify PID executable, then `kill 76822`; check exact-target pgrep and all pre-existing PIDs. | Exact-target pgrep exits **1**, no remaining target process. Installed PID **51217** remains running with the original `/nix/store/…/ReShiki.app/Contents/MacOS/reshiki` path. No other pre-existing ReShiki processes existed in the baseline. | **PASS** |

**Frame comparison and interpretation**

Measurements scan stored PNG RGB values exactly equal to **(62,133,117)**, using qualifying canvas rows/columns with more than 400 matches. Handles are excluded by the long-run criterion. The full measurement table, including partial clipped frames and later zoom states, is in [frame-measurements.md](frame-measurements.md), with raw values in [frame-measurements.json](frame-measurements.json).

| Screenshot | Left stroke | Top stroke | Right stroke | Bottom stroke | Geometric width × height | Center |
|---|---|---|---|---|---|---|
| A03 before | 664..665 | 603..604 | 1548..1549 | 1487..1488 | 884 × 884 px | (1106.5,1045.5) |
| A06 back to 3D | 664..665 | 603..604 | 1548..1549 | 1487..1488 | 884 × 884 px | (1106.5,1045.5) |
| A07 Original drawing | 664..665 | 603..604 | 1548..1549 | 1487..1488 | 884 × 884 px | (1106.5,1045.5) |
| A06 − A03 | 0 px | 0 px | 0 px | 0 px | 0 × 0 px | (0,0) px |
| A07 − A03 | 0 px | 0 px | 0 px | 0 px | 0 × 0 px | (0,0) px |

Widths/heights are distances between corresponding edges of opposite strokes, consistent with the expected 744/884 px dimensions. Inclusive outer footprints are 746/886 px because they include the full two-pixel strokes. Centers above are zero-based raster-index stroke centers; the measurement report states the physical-pixel-center conversion. The later 97% geometry is reported separately and is not compared as if it were at 122%.

Visual comparison was independently reviewed against A05 and A08: A11, A12, A14, and A16 all retain the upright original ring layout and carbonyl directions. A05 has the diagonally compressed projection. This comparison is based on visible layout, not only the active option label.

**Anything not verified, and why**

- **Native Edit → Redo:** unavailable in this build’s exposed native menu bar. Toolbar Redo enabled state was verified at A09, and actual Cmd+Shift+Z behavior was exercised at A14 and A15.
- **Redo of the earlier switch-to-2D display after choosing Both:** that redo branch no longer exists. A13→A14 confirms the available Redo at the A09-like state restores Both. The additional A15b→A15→A16 sequence verifies redo of the underlying Original drawing restoration and its resulting Both layout instead.
- **Hidden 2D coordinates while only 3D is shown:** they cannot be viewed directly in A07. A08 and the subsequent Both views demonstrate the restoration, including through the restoration’s Undo/Redo history entry.
- **Full selected Both-frame width and center:** the right frame edge is outside the visible canvas at both requested Both zooms. Only visible strokes are measured; missing edges are not inferred. A12 supplies a deselected view of both complete molecules at 97%.
- **Compact A05 selection geometry under the mandated >400 exact-pixel rule:** the visible compact frame does not qualify. Some later 2D/97% strokes contain only one row/column of the exact target RGB; their measured thicknesses are recorded without inventing antialiased edges.
- **Initial setup fidelity:** keyboard drawing was initially active, so the first resize did not act on the frame. F8 exited that mode, and the exact prescribed resize drag then succeeded. The failed attempt was retained. The initial arrow and 3D activations were AX actions, so their caller-level x/y coordinates do not exist; all coordinate-driven actions and drags are documented above.
