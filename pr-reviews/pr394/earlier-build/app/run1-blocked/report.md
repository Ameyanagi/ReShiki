# ReShiki #386 phase 1b — local UI verification

**Overall verdict: FAIL — verification blocked by computer-use attachment failure.**

No conclusion about the correctness of the 3D feature can be drawn from this run. The requested interaction sequence could not begin because the available computer-use connector could not attach to the already-running evidence app. No mouse or keyboard input was sent.

## Target and safety checks

- Requested application: **ReShiki 1b Evidence**
- Requested bundle identifier: `dev.reshiki.verification.ev3861b`
- Requested process: **PID 28684**
- Requested capture window: **CGWindow 7000**
- The computer-use application inventory reported this exact application and bundle identifier as running. It also listed the separate installed ReShiki; that application was not selected or interacted with.
- Read-only process inspection, `ps -p 28684 -o pid=,comm=,args=`, confirmed PID 28684 was running the branch binary at `/Users/shiranui/dev/ReShiki/.claude/worktrees/wf_228c0944-39d-1/target/debug/reshiki`, with the caffeine input file.
- No application was successfully launched, relaunched, closed, or quit. No system settings or repository files were changed. The drawing was not saved, and the window was not resized.
- `morph-go` was not created, because the 3D click could not be performed.

## Attachment blocker

The following supported or exposed computer-use entry points were attempted:

1. `cua.getApp('dev.reshiki.verification.ev3861b')`
   - Failed with `NSCocoaErrorDomain Code=259`.
   - The tool reported that the registered wrapper at `/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps1b/ReShiki 1b Evidence.app/` could not be opened.
   - Underlying error: `NSOSStatusErrorDomain Code=-10827`, `kLSNoExecutableErr: The executable is missing`.
   - Although intended to bind to the application already shown as running, this connector entry point attempted to resolve/open its registered bundle and failed. No successful app attachment was obtained.
2. `cua.getApp('ReShiki 1b Evidence')`
   - Failed with `Invalid app: ReShiki 1b Evidence`.
3. `cua.getApp({ windowId: 7000 })`
   - Failed with `macOS getApp requires an app name, path, or bundle ID.`

The available macOS computer-use interface did not expose a PID attachment method. Further interaction was stopped to preserve the restriction against touching the separate installed ReShiki or launching an application.

## Captured baseline

**Screenshot:** [00-blocked-initial.png](00-blocked-initial.png)

Captured successfully using the requested window-specific shell command:

```sh
screencapture -x -o -l 7000 "/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1b/app/00-blocked-initial.png"
```

- **Window title:** `Untitled • — ReShiki`
- **Tab title:** `Untitled`, with a green dot.
- **Bottom-left status:** `Structure imported · Undo restores the previous drawing`
- **Visible state:** A 2D caffeine drawing is present. The Select arrow has a highlighted background. There is no selection rectangle. The Properties panel says `Select an object to edit its properties.` The Molecular properties card shows `Whole drawing · C8H10N4O2` and `14 atoms · 15 bonds`. Zoom is `238%`.
- **Visual observations:** No obvious molecule clipping or overlapping labels is visible in this initial state. An orange crosshair marker is visible beside the molecule. No 3D model or floating Show as bar is displayed. This is an unselected baseline, not the requested selected-2D evidence.
- **Pointer handling:** The connector could not attach, so the mouse pointer could not be moved before capture. The requested capture command was used without a cursor-inclusion option.
- **Baseline verdict:** PASS for obtaining the specified window capture; this does not validate any 3D interaction.

## Requested steps

In the table below, **FAIL — blocked** means that the verification could not be executed. It does **not** mean the application feature failed. Only the baseline above was captured. Requested filenames below are recorded for traceability; those files were not created.

For every unexecuted step, the step-specific window title and status bar text are **not observed**. The only directly observed title and status are those recorded for `00-blocked-initial.png`; they are not represented as evidence for later states.

| Step | Requested action and expected result | What was done / observed | Evidence | Verdict |
| --- | --- | --- | --- | --- |
| 1 — Select 2D | Activate the evidence window, choose Select, click empty canvas, and press Cmd+A. Expect the whole caffeine molecule selected and Structure → Show as with 2D active, 3D, and Both. | App attachment was attempted and failed as documented above. No selection input was sent. The baseline shows the unselected 2D drawing; Structure controls for a selected molecule were not verified. Step-specific title/status: not observed. | Requested `01-selected-2d.png` not created. Baseline: `00-blocked-initial.png`. | **FAIL — blocked** |
| 2 — Generate and morph | Touch `morph-go` immediately before clicking Structure → 3D. Wait for generation to finish and capture the selected 3D model with its floating bar. | Neither the trigger file creation nor the click was performed. Conformer generation, Generating status, morph animation, and final 3D state were not observed. Step-specific title/status: not observed. | `02-3d-shown.png` not created. | **FAIL — blocked** |
| 3 — Floating bar | Capture the bar and note whether it is above/below the selection and which button is active. | No 3D model was entered. Bar visibility, position, active button, overlaps, and clipping could not be assessed. Step-specific title/status: not observed. | `03-bar.png` not created. | **FAIL — blocked** |
| 4 — Rotate during drag | Press inside the model frame near centre-left, move about 120 px right and 40 px down in several steps, capture while held, then release. Expect a turned model and hidden bar while dragging. | No drag was attempted. The documented computer-use API exposes a complete `drag(from, to)` operation but no separate mouse-down/move/mouse-up methods; even if attachment had succeeded, holding a drag across the shell capture was not available through the documented interface. The allowed one-go fallback also could not be executed because attachment failed. Step-specific title/status: not observed. | `04-rotate-mid-drag.png` not created. | **FAIL — blocked** |
| 5 — Rotation released | Capture after release. Expect the new orientation retained, floating bar visible, and one rotation undo step. | No preceding rotation occurred. Retained orientation, bar reappearance, status, and undo granularity were not verified. Step-specific title/status: not observed. | `05-rotate-released.png` not created. | **FAIL — blocked** |
| 6 — Shift-roll | Hold Shift and drag in an arc inside the frame, then capture. Expect in-plane rotation relative to step 5. | No Shift-drag was attempted. Roll behavior was not verified. Step-specific title/status: not observed. | `06-shift-roll.png` not created. | **FAIL — blocked** |
| 7 — Double-click reset | Double-click the model inside its frame. Expect a return to the opening orientation from step 2. | No double-click was attempted. Reset behavior and orientation comparison were not verified. Step-specific title/status: not observed. | `07-double-click-reset.png` not created. | **FAIL — blocked** |
| 8 — Rotate again | Perform another plain interior drag so the orientation differs from the opening view, then capture. | No drag was attempted and no rotated reference state was established. Step-specific title/status: not observed. | `08-rotated-before-resize.png` not created. | **FAIL — blocked** |
| 9 — Resize | Drag the bottom-right selection corner about 100 px right and 100 px down. Expect a larger model/frame, corner handles, and no edge handles. | No resize was attempted. Frame size, model scaling, handle visibility, and clipping were not verified. Step-specific title/status: not observed. | `09-resized.png` not created. | **FAIL — blocked** |
| 10 — Layout and flatten | Choose Structure → 2D layout → From the 3D view and capture; then choose Show as → 2D and capture. Expect the turned 3D arrangement flattened into 2D, with distant bonds lighter. | Neither control was operated. The layout choice, projected atom arrangement, depth-dependent bond appearance, and resulting status were not verified. Title/status for both requested shots: not observed. | `10-layout-from-3d.png` and `11-flattened-2d.png` not created. | **FAIL — blocked** |
| 11 — Undo flatten | Press Cmd+Z once. Expect the 3D state from steps 9/10 restored in a single undo step. | Cmd+Z was not sent. Undo behavior and restored orientation/frame size were not verified. Step-specific title/status: not observed. | `12-undo.png` not created. | **FAIL — blocked** |
| 12 — Optional context menu | Right-click the molecule, open Show as, capture the submenu, then press Escape. | No context menu was opened. Its options, including 2D from the 3D view, were not verified. Step-specific title/status: not observed. | `13-context-menu.png` not created. | **FAIL — blocked; optional** |

## Unverified items and reason

All requested 3D feature checks remain unverified: whole-molecule Structure controls, conformer generation, the 0.75-second morph, floating-bar placement and active state, drag-time hiding, trackball rotation and undo granularity, Shift-roll, double-click reset, corner-only resize handles, model scaling, projection to 2D, depth shading, single-step flatten undo, and the Show as context submenu. Interaction-dependent lag, residual labels, clipping, and overlaps also remain unassessed.

The blocking condition was the computer-use connector's inability to bind to the existing evidence application on macOS. The branch process remained available for the requested window-specific screenshot. No application repair, bundle modification, rebuild, relaunch, or alternate UI automation was performed.
