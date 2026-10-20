# ReShiki #386 phase 2 — local UI verification

Verified on macOS on 2026-10-20, approximately 18:39–18:48 JST. The requested style changes, Glossy/Flat change, single-drag Undo grouping, and sphere shading at displayed **300%** and **400%** passed visual verification. The native **Edit menu/Undo label could not be verified because this build has no Edit menu**. Two additional visual inconsistencies were observed: a stale Structure subtitle and fine seams across some carbon–carbon bond cylinders.

## Process isolation and capture setup

- Launched exactly the supplied `open -n` command for `ReShiki386P2Evidence.app`, with the supplied `RESHIKI_DATA_DIR` and `--open` drawing path.
- `pgrep -f 'wf_228c0944-39d-2/target/debug/reshiki --open'` returned exactly **74366**. This differs from the protected installed-app PID **51217**.
- Saved the launched command and start time in [launched-process.txt](launched-process.txt).
- The supplied window helper returned **7185**; it still returned 7185 at the end of the UI checks.
- Computer use attached only by bundle ID **dev.reshiki.verification.ev3862p2**. Target window title: `caffeine-ballstick-flat.rsk — ReShiki`, with a dirty-state dot during changes.
- Read-only window inspection confirmed frontmost PID **74366**, the expected bundle ID, and window **7185** owned by that PID. See [window-verification.txt](window-verification.txt).
- Actual window frame was **1280 × 852 points**, origin **(200, 115)**; captures are **2560 × 1704 pixels**, at 2× Retina scale. This was larger than the approximate size in the task. The window was never resized or moved.
- The entire 3D model section through **Reset view** fitted without scrolling or manually collapsing Properties sections. Selecting the model automatically changed Molecular properties from expanded to collapsed.
- Window evidence used `screencapture -x -o -l 7185 "$ART/<filename>.png"`. Only `07b-edit-menu-full-screen.png` used the authorized full-screen capture. The pointer was parked on the title bar before window captures, except image 08, where it remained at the app menu, also away from the model and documented controls.
- No Save command was issued. Repository files and system settings were not modified. The installed app was never attached to, focused, clicked, opened, or quit. Other visible system alerts were not dismissed.

## Step-by-step results

For steps 1–9, unless explicitly changed below: **Hydrogens All**, **Perspective 20%**, **Background None**, and **3D** remained active. The inspector reported **14 atoms · 15 bonds**. Buttons for Polar/None hydrogens, Soft card, and Reset view were visible.

| Step | Action and expected result | Observed result, active controls, and exact status | Evidence | Verdict |
|---|---|---|---|---|
| 1 | Clicked Select/move, then Cmd+A to select the model. Expected the 3D model section and initial Ball & stick state. | Whole section visible. **Ball & stick**, **Flat · journal**, Depth fade **35%**; style pictograms present. Separate outlined spheres and shaded cylinders. Status **“Document opened”**; zoom **201%**. | [01-section-ballstick.png](01-section-ballstick.png) | PASS |
| 2 | Clicked **Stick**. Expected Stick active and corresponding model rendering. | **Stick** active; 3D model summary says “Stick”. Large atom spheres disappear; cylinders meet at small rounded joints. Flat and fade **35%** remain. Status **“Drawing updated”**; **201%**. Structure subtitle inconsistency noted below. | [02-style-stick.png](02-style-stick.png) | PASS for requested switch |
| 3 | Clicked **Wireframe**. Expected Wireframe active and line rendering. | **Wireframe** active; thin colored lines replace spheres and cylinders. Flat and fade **35%** remain. Status **“Drawing updated”**; **201%**. | [03-style-wireframe.png](03-style-wireframe.png) | PASS for requested switch |
| 4 | Clicked **Space-filling**. Expected large overlapping spheres. | **Space-filling** active; large overlapping spheres obscure bonds. Flat and fade **35%** remain. Status **“Drawing updated”**; **201%**. | [04-style-spacefilling.png](04-style-spacefilling.png) | PASS for requested switch |
| 5 | Clicked **Ball & stick** again. Expected original style restored. | **Ball & stick** active; original sphere/cylinder appearance and selection bounds restored. **Flat**, fade **35%**. Status **“Drawing updated”**; **201%**. | [05-style-ballstick-again.png](05-style-ballstick-again.png) | PASS |
| 6 | Clicked **Glossy · slides**, captured it, then clicked **Flat · journal** again. | In the shot, **Glossy** is active, spheres have stronger highlights and lose their dark outlines; **Ball & stick**, fade **35%** remain. Status **“Drawing updated”**; **201%**. Subsequent Flat click was verified through the active toggle and screenshot observation before the drag. | [06-look-glossy.png](06-look-glossy.png) | PASS |
| 7 | Starting at **35%**, used the supplied helper for exactly one held-button drag, **(1095, 645) → (1222, 645)** in window-relative points. Expected roughly 85–90% and visible depth fading. | Depth fade became **87%**. **Ball & stick / Flat** remained active. Blue and red atoms became visibly paler than in 05/08. Status **“Drawing updated”**; **201%**. The nearly planar projection limits independent identification of which atoms are farthest back. | [07-depth-fade-dragged.png](07-depth-fade-dragged.png) | PASS for value and visible fading |
| 7b | Inspected the native menu bar for **Edit > Undo**, then attempted to leave the menu with Escape. | The verified app exposes only its application menu; **no Edit menu exists**, and no native Undo label was available. Accessibility showed About, Services, Hide, Hide Others, Show All, and Quit. The full-screen shot shows the active verification-app menu-bar name and no Edit heading. It also contains Finder alerts; these were untouched. Escape did not clear the menu's accessibility state, so the exposed **Cancel** menu action was used after the first Undo. | [07b-edit-menu-full-screen.png](07b-edit-menu-full-screen.png) | NOT VERIFIED — menu unavailable |
| 8 | Pressed **Cmd+Z once**. Expected the whole drag to undo to 35%, keeping Flat and Ball & stick. | Depth fade **35%**, **Flat · journal**, **Ball & stick**. Stronger atom colors restored. Status **“History restored”**; **201%**. | [08-undo-slider.png](08-undo-slider.png) | PASS |
| 9 | Pressed **Cmd+Z once more**. Expected undo of the prior Flat click, rather than another intermediate slider value. | **Glossy · slides** became active; Depth fade stayed **35%**, style stayed **Ball & stick**. Glossy highlights returned. Status **“History restored”**; **201%**. Together with step 8, this verifies that the drag occupied one Undo step. | [09-undo-again.png](09-undo-again.png) | PASS |
| 10 | Pressed **Cmd+Shift+Z once**, then clicked empty canvas at computer-use screenshot pixel **(350, 450)**. | Redo restored **Ball & stick / Flat / Depth fade 35%**, verified before deselection. Selection rectangle disappeared; Properties said **“Select an object to edit its properties.”** Status **“History restored”**; **201%**. | Live screenshot/AX observation; following images show deselection | PASS |
| 11a | Zoomed to displayed **300%**, panned with scroll, parked pointer, waited **1 second**, and captured. Details below. | Exact displayed zoom **300%**; status **“History restored”**. Several large spheres and bonds fill the canvas. Spheres show continuous smooth shading with no obvious stepped rings/banding. The bottom hydrogen is partly clipped by the viewport. Fine bond seams are noted separately below. | [10-zoom-300-ballstick.png](10-zoom-300-ballstick.png) | PASS for zoom and sphere shading |
| 11b | Used another Cmd+scroll gesture to displayed **400%**, parked pointer, waited **1 second**, and captured. | Exact displayed zoom **400%**; status **“History restored”**. Enlarged spheres have smooth gradients and crisp outlines, without obvious stepped rings/banding. The enlarged molecule extends beyond the top/right/bottom viewport edges as expected for this close-up. | [11-zoom-400-ballstick.png](11-zoom-400-ballstick.png) | PASS for zoom and sphere shading |
| 12 | Verified the launched command still matched the target, then executed **kill 74366**. Checked disappearance and protected PID. | Target `pgrep` produced no output, exit **1**. `ps -p 74366` exited **1**. `ps -p 51217` exited **0**, showing the original installed app and its original start time. | [process-cleanup.txt](process-cleanup.txt) | PASS |

## Exact drag and zoom interactions

The single Depth fade drag was:

```sh
/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b-drag 74366 1095 645 1222 645
```

The helper returned `ok window 7185 origin 200.0,115.0`. Coordinates were derived from the 2× computer-use screenshot: the 35% knob was approximately pixel **(2189,1290)**, corresponding to point **(1094.5,645)**. Only one slider drag was performed.

For zoom, clicking the displayed percentage did not open an editable field. Clicking status-bar **+** at screenshot pixel **(2423,1662)** changed **201% → 251%**. The View options row was briefly opened at **(2176,1662)** and closed at the same point; no option was changed. A read-only source inspection confirmed that the percentage is a text label and +/- multiply the zoom, rather than offering exact entry.

The artifact [cmd-scroll.swift](cmd-scroll.swift) implements the requested Cmd+scroll interaction using real CGEvents. It guards the exact verification PID/bundle, verifies window ownership, and checks that the pointer destination is not covered by another process's window. Its coordinates are window-relative **points**. The working event sequence was:

```sh
swift cmd-scroll.swift 74366 200 700 30   # displayed 251% → 300%
swift cmd-scroll.swift 74366 200 700 48   # displayed 300% → 400%
```

Between these two zoom gestures, computer use panned at screenshot pixel **(400,1400)** with `scroll(up, 0.15)` followed by `scroll(down, 0.1)`. The status remained **300%**. Pointer parking on the title bar and a one-second wait preceded each final zoom capture.

Automation retries: an initial process-directed scroll attempt left zoom at 251%; the working helper uses the real event tap. One guard also rejected the macOS cursor pseudo-window before sending any events; the guard was refined to ignore only that cursor window. These attempts did not change document contents. One early stale accessibility ID was rejected and refreshed before the Stick action succeeded.

## Additional findings

1. **Stale Structure subtitle — visual inconsistency.** Images 02–04 still say **“Ball & stick · Chemistry and atom IDs stay the same”** in the Structure card while the Style toggle, 3D model summary, and canvas correctly show Stick, Wireframe, or Space-filling. The requested switches pass, but the summary text is inconsistent.
2. **Fine bond-cylinder seams — visual artifact.** At 300% and especially 400%, a thin pale transverse line crosses the central carbon–carbon cylinder; another is visible on the lower diagonal carbon–carbon cylinder. These are isolated bond seams, separate from sphere shading. The spheres themselves did not show obvious stepped concentric rings.
3. **Full-screen evidence has unrelated alerts.** The allowed full-screen capture includes Finder alerts, including “The application ‘ReShiki’ is not open anymore.” Their origin was not investigated. They were not clicked or dismissed. The target remained running and responsive, with its PID/bundle independently verified. The right-side slider path was clear, and window-only evidence excludes the unrelated alert windows. The entire desktop was therefore not visually unobstructed, although the actual drag path was unobstructed.

## Unverified items and limits

- A native Edit-menu Undo description could not be inspected because the target build has no Edit menu. The in-app toolbar exposes the generic accessibility label **“Undo · ⌘Z”**; history behavior was verified directly with the two Undo steps.
- Increased fade visibly lightened the model, but the nearly planar pose does not independently establish the exact depth ordering of every atom.
- Hydrogens Polar/None, Perspective adjustment, Soft card, and Reset view were verified as present only; the requested flow did not exercise them. Individual Undo behavior of each Style/Hydrogen/Background control was not exhaustively tested.
- The zoom shots deliberately deselect the model, so their inspector does not display the Look or slider state. **Ball & stick / Flat / 35%** was verified immediately before deselection, and only view interactions followed.
- Smooth shading is a visual result for this caffeine model at the captured zoom levels. Internal image-tile implementation and all possible orientations/models were not separately tested.
- All required evidence filenames were saved. The pre-existing `00-preflight-launch.png` and other pre-existing artifacts were left in place and were not treated as evidence from this run.
