**ReShiki #386 phase 1b — local UI verification**

The requested core flow passed: first-use 3D generation and intermediate morph states, floating display bar, trackball rotation, Shift-roll, double-click reset, corner resizing, flattening from the turned 3D view, and restoration with one Undo. One visual defect was observed: the floating bar's lower border is clipped at the canvas/footer boundary after the first rotation (`05-rotate-released.png`). Its buttons remain readable.

**Run identity and safeguards**

- Test bundle: `/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps1b2/ReShiki1bEvidence.app`
- Bundle identifier: `dev.reshiki.verification.ev3861b2`.
- Launched PID: **46018**. Window ID: **7018** throughout. Launch details are in [launched-process.txt](launched-process.txt).
- The existing installed app was PID **51217**, executing `/nix/store/lx488j7ggxfy04cgck5afam7qh13fk3s-reshiki-0.12.0/Applications/ReShiki.app/Contents/MacOS/reshiki`. Its process record is in [installed-process-before.txt](installed-process-before.txt). It was never selected, focused, clicked, opened, or quit.
- Computer use attached by the exact verification bundle identifier. All observed app states belonged to `ReShiki1bEvidence`. Every drag helper invocation reported window `7018`, origin `200.0,115.0`.
- The window was kept at its initial size. Saved images are **2560 × 1704 pixels**, corresponding to approximately **1280 × 852 points** at Retina 2×. Zoom remained **238%**. No drawing was saved, repository files were not edited, and system settings were not changed.
- All ordinary evidence images used `screencapture -x -o -l 7018 "$ART/<name>.png"`. The held-drag image used the supplied drag helper's capture option.
- Before ordinary captures the pointer was moved to the title bar by clicking screenshot coordinate `(1500,30)`, away from the molecule. Exceptions were the requested held-drag capture and the open context submenu.
- The footer's persistent left-hand recovery UI read **“Recovery draft”**, **“Restore”**, **“Dismiss”** in every evidence image. It was left untouched. The operation status text beside it is recorded below.

Launch command executed:

```sh
open -n "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps1b2/ReShiki1bEvidence.app" --env RESHIKI_DATA_DIR=/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-data --args --open "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b/input/caffeine.mol"
```

The exact requested `pgrep -f 'apps1b2/ReShiki1bEvidence.app/Contents/MacOS/reshiki'` returned one PID, `46018`. The supplied window helper returned `7018`. An initial stale accessibility-element error was resolved by refreshing the accessibility state; it did not change the document or redirect interaction to another app.

**Step-by-step observations**

1. **Select the whole 2D molecule — PASS.** Raised the verification window, clicked the Select arrow, clicked empty canvas at computer-use screenshot coordinate `(900,500)`, and pressed Cmd+A. Caffeine was selected, with **14 atoms · 15 bonds**. Properties → Structure was already visible and showed **2D active**, with 3D and Both available. No Properties scrolling was needed. The drawing had the expected original caffeine layout. A floating bar was also visible in 2D; this was additional behavior, not a failure of the requested 3D test.

   Evidence: [01-selected-2d.png](01-selected-2d.png). Window title: **“Untitled • — ReShiki”**. Status: **“Structure imported · Undo restores the previous drawing”**. No blocking defect observed.

2. **First 3D switch and morph capture — PASS; exact duration not measured.** Wrote `46018` to `$ART/morph-go`, then immediately issued the computer-use click on **3D in Properties → Structure**. The marker was retained. The background script produced all 40 frames in [morph-raw](morph-raw), and its [log](morph-raw/log.txt) confirms PID `46018`, WID `7018`. Generation completed and the selected ball-and-stick model appeared. The floating bar was visible below the frame with 3D active.

   Evidence: [02-3d-shown.png](02-3d-shown.png), plus `morph-raw/f01.png` through `f40.png`. Window title: **“Untitled • — ReShiki”**. Final status: **“Shown as 3D model · Undo restores the 2D drawing”**. Intermediate motion and timing limits are detailed below.

3. **Floating display bar — PASS.** Captured the same settled selection. The bar sits **below** the selection frame, where there is more space than above it. It contains Show as / 2D / 3D / Both; **3D has the pale-green active background**. The Structure section also has 3D active.

   Evidence: [03-bar.png](03-bar.png). Window title: **“Untitled • — ReShiki”**. Status: **“Shown as 3D model · Undo restores the 2D drawing”**. No clipping of this bar in this initial view.

4. **Trackball rotation while held — PASS.** With the test window frontmost, used the supplied helper from **(450,440) → (570,480) points**, a movement of **+120,+40**, starting well inside the frame. The helper captured before releasing the button. The model turned to a narrow, nearly edge-on view, and the floating bar was **hidden**. A canvas instruction strip read **“Drag to rotate · Shift-drag to roll · Double-click to reset”**.

   Evidence: [04-rotate-mid-drag.png](04-rotate-mid-drag.png). Window title: **“Untitled • — ReShiki”**. Status while held: **“Shown as 3D model · Undo restores the 2D drawing”**. No rotation failure observed.

5. **Rotation after release — PASS for persistence/bar return; FAIL for complete bar visibility.** The rotated orientation remained after release and the floating bar reappeared below the selection, with 3D active. However, the lower rounded border of the bar runs into and is clipped by the canvas/footer boundary. The labels and active button remain readable. This is a visual layout issue; the bar did return as expected.

   Evidence: [05-rotate-released.png](05-rotate-released.png). Window title: **“Untitled • — ReShiki”**. Status: **“Drawing updated”**.

6. **Shift-roll — PASS.** Used the helper with Shift held from **(570,480) → (490,380) points**, movement **−80,−100**, starting right of the model's center and remaining within the frame. The previously almost vertical model rolled to a diagonal from upper-left to lower-right, with the same nearly edge-on appearance. The bar was visible after release.

   Evidence: [06-shift-roll.png](06-shift-roll.png). Window title: **“Untitled • — ReShiki”**. Status: **“Drawing updated”**. No defect observed for this action.

7. **Double-click reset — PASS.** Double-clicked the model at computer-use screenshot coordinate **(975,1000)**, equivalent to approximately **(487.5,500) points**. The molecule and selection frame returned to the opening orientation and size seen in image 02. The orange active-position indicator and keyboard-context text changed, but the molecular orientation matched.

   Evidence: [07-double-click-reset.png](07-double-click-reset.png). Window title: **“Untitled • — ReShiki”**. Status: **“Drawing updated”**. No reset failure observed.

8. **Second rotation before resizing — PASS.** Used a different direction with the helper: **(550,460) → (475,400) points**, movement **−75,−60**. The resulting model was visibly tilted from the opening view, with a wider, shallower projection of the rings. The floating bar returned after release.

   Evidence: [08-rotated-before-resize.png](08-rotated-before-resize.png). Window title: **“Untitled • — ReShiki”**. Status: **“Drawing updated”**. No defect observed for this action.

9. **Bottom-right corner resizing — PASS.** Dragged the bottom-right corner using the helper from **(807,680) → (907,760) points**, movement **+100,+80**. Both endpoints were within the canvas. The frame and molecule grew, retaining the turned orientation. The approximate frame changed from `(190,253)–(807,680)` to `(190,253)–(912,752)` points, with the top-left fixed and proportional scaling. Four corner resize handles were visible; **no edge-midpoint resize handles** were shown. A separate top-center circular rotation handle remained visible.

   Evidence: [09-resized.png](09-resized.png). Window title: **“Untitled • — ReShiki”**. Status: **“Drawing updated”**. The model and bar fit within the canvas in this state.

10. **Choose the 3D-derived layout and flatten — PASS.** In Properties → Structure, clicked **From the 3D view** under **2D layout**. It became active and displayed the explanation **“Flatten the model as it is turned now, with far bonds lighter.”** The model stayed in its resized, turned state.

    Evidence before flattening: [10-layout-from-3d.png](10-layout-from-3d.png). Window title: **“Untitled • — ReShiki”**. Status: **“Drawing updated”**. Structure and both layout choices are visible.

    Then clicked **2D in the Structure section**. The model became a 2D drawing whose ring and substituent arrangement follows the turned 3D projection and differs from the original image 01. Far portions of the drawing, including the upper-left oxygen/bonds, are visibly lighter than the nearer lower-right portions. The selection bounds contracted when returning to the 2D drawing; the 3D frame size was not retained as the drawing bounds. No requirement in the requested flow specified identical 2D/3D screen size.

    Evidence after flattening: [11-flattened-2d.png](11-flattened-2d.png). Window title: **“Untitled • — ReShiki”**. Status: **“Shown as 2D from the 3D view · Undo restores the 3D model”**. No mismatch in the expected projection or depth appearance observed.

11. **One Undo — PASS.** Pressed **Cmd+Z exactly once** after flattening. The resized 3D molecule and frame returned, matching images 09/10 in orientation, size, and placement. **From the 3D view** remained active, 3D was active, and the floating bar was visible.

    Evidence: [12-undo.png](12-undo.png). Window title: **“Untitled • — ReShiki”**. Status: **“History restored”**. No restoration mismatch observed.

12. **Optional context submenu — PASS for menu contents; hover-only behavior not independently tested.** Right-clicked the molecule at computer-use screenshot coordinate **(1215,950)**, then clicked **Show as** to expand its submenu. The available computer-use API did not expose a hover operation, so a submenu-row click was used. The menu showed **2D drawing**, **2D from the 3D view**, **3D model** (disabled/current), and **2D and 3D**. Pressed Escape afterward and confirmed the menu closed without changing the drawing.

    Evidence: [13-context-menu.png](13-context-menu.png). Window title: **“Untitled • — ReShiki”**. Status: **“History restored”**. No unexpected menu entry or missing display choice observed.

**Exact drag-helper commands**

All coordinates below are window-relative **points**, including the title bar. Each helper was run while the verification window was active and unobstructed. These were the only drag operations.

```sh
/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag 46018 450 440 570 480 capture=/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b/app/04-rotate-mid-drag.png
/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag 46018 570 480 490 380 shift
/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag 46018 550 460 475 400
/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag 46018 807 680 907 760
```

**Morph-frame evidence**

All 40 frames have window title **“Untitled • — ReShiki”** and the persistent recovery UI described above.

| Frames | Observed state | Operation status text |
|---|---|---|
| f01–f26 | Selected original 2D drawing, unchanged across these samples | Structure imported · Undo restores the previous drawing |
| f27 | Still 2D geometry; 3D selected; Properties reports “Generating the conformer · ETKDG, then MMFF94s” | Generating 3D model · MMFF94s |
| f28 | Spheres/sticks appear around the original drawing positions; final hydrogen geometry not yet present | Shown as 3D model · Undo restores the 2D drawing |
| f29–f32 | Distinct intermediate atom positions; hydrogens emerge during the transition, visibly translucent in f30 | Shown as 3D model · Undo restores the 2D drawing |
| f33–f40 | Settled final 3D geometry matching image 02 | Shown as 3D model · Undo restores the 2D drawing |

The logged samples occur about every 120–145 ms. The first change falls between f27 (`1792487051846`) and f28 (`1792487051978`); settling falls between f32 (`1792487052516`) and f33 (`1792487052657`). This is consistent with a transition lasting about **538–811 ms**, which includes the expected **750 ms**. The evidence verifies intermediate geometry rather than a simple before/after replacement, but cannot measure the precise animation duration or frame-rate smoothness.

Additional transient observations: the representation changes from text/lines to spheres/sticks between f27 and f28, then positions interpolate; the selection frame expands and the bar moves from above to below; the Molecular properties formula temporarily disappears during f28–f30 and returns by f31. The final orange active-position marker overlaps the top hydrogen. These did not prevent the tested interactions. No persistent blank model or obviously broken bonds were observed.

**Final process cleanup**

**PASS.** Immediately before cleanup, `ps` still identified PID `46018` as the exact verification-bundle executable, and the window helper still returned `7018`. Executed **`kill 46018` only**, which succeeded with exit code 0. No app menu was used.

- Verification process check: `pgrep -f 'apps1b2/ReShiki1bEvidence.app/Contents/MacOS/reshiki'` returned **no output, exit code 1**. The test process was gone.
- Installed app check: `pgrep -fl '^/nix/store/[^ ]*/Applications/ReShiki[.]app/Contents/MacOS/reshiki([[:space:]]|$)'` returned **PID 51217**, with the same installed executable path recorded before the run, **exit code 0**. The user's installed app remained running.
- No further computer-use interaction occurred after terminating the verification process.

The exact cleanup results are also recorded in [process-cleanup.txt](process-cleanup.txt).

**Unverified items and limits**

- Exact 0.75-second duration and per-frame animation smoothness: the background capture sampled approximately every 120–145 ms.
- Whether every rotation produces exactly one Undo entry: the specified flow tested one Undo for flattening, not individual rotation-history boundaries.
- Hover-only opening of the context submenu: the available computer-use API exposed click and keyboard actions but no hover operation; the submenu was expanded by clicking its parent row.
- Both mode, alternative molecules, repeated conformer generation, saving/reopening, and persistence across launches were outside the requested flow and were not exercised.
