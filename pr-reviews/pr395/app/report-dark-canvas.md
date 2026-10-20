# Dark-canvas UI check

Performed on 2026-10-20 (JST) using computer use and shell.

## Isolation and launch

- Launched only `/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/apps386-final-2/ReShiki386F2Evidence.app` with `open -n`, the requested `RESHIKI_DATA_DIR=…/ev386-2-final-data-dark`, and `--open …/ev386-2-final/input/own/caffeine-stick-flat.rsk`.
- The exact bundle executable pattern returned one PID: **46469**. The supplied window-ID helper returned **7602**. Process details were appended to `launched-process.txt`.
- Attached computer use by bundle ID **dev.reshiki.verification.ev386final2**. All UI interactions and screenshots targeted that verification app. The installed ReShiki at PID **51217** was not focused, clicked, captured, or terminated.

## Canvas control and actions

The app chrome initially appeared dark while the drawing canvas was white. Clicking the **sun icon in the bottom status bar, immediately left of “View”**, changed the canvas to black. The icon became a moon; its tooltip read **“Dark canvas · Switch to light”**. The status bar read **“Dark canvas · Copies retain ink colors on a transparent background”**. No system settings or command palette were used.

Clicked the Select tool, then clicked the molecule. Captured the selected stick model. Clicked empty canvas to deselect, then clicked a blank part of the title bar to park the pointer there before the second capture. Reselected the model, clicked **3D model → Style → Ball & stick**, and confirmed the accessibility state changed to Ball & stick on / Stick off. Deselected on empty canvas and parked the pointer on the title bar again before the third capture.

All requested screenshots were saved with `screencapture -x -o -l 7602` and opened for visual verification:

| File | Verified contents |
| --- | --- |
| `C03-dark-canvas-stick-selected.png` | Black canvas; selected caffeine stick model with cyan bounding box, corner handles, and rotation handle. Inspector shows Stick, Flat · journal, and Hydrogens All selected. |
| `C04-dark-canvas-stick-deselected.png` | Same stick model on black canvas with no selection box or floating selection controls. Inspector prompts to select an object. Pointer parked on the title bar. |
| `C05-dark-canvas-ballstick.png` | Deselected caffeine ball-and-stick model on black canvas; larger atom spheres and thinner connecting sticks clearly distinguish the new style. Pointer parked on the title bar. |

## Observations

- White/light-gray hydrogens are clearly visible against black in both styles. Gray carbons, blue nitrogens, and red oxygens remain distinguishable.
- The cyan selection outline and handles have good contrast. Dark shading/edge outlines on individual atoms and bonds have less contrast against black, but no missing atoms or obvious clipping were observed.
- Keyboard drawing mode was already active when the file opened, despite the Select tool also being active. An orange keyboard-position crosshair remained visible: on the molecule in the selected capture and at the last empty-canvas click in the deselected captures. It remained after parking the pointer on the title bar, so it is a persistent drawing marker rather than the mouse pointer or a remaining selection box.
- Switching the canvas theme immediately added the document’s unsaved-change marker. The style change was also left unsaved. No Save or Save As command was invoked, and the source drawing was not saved.

## Cleanup

Sent `kill 46469` only. A subsequent exact-bundle `pgrep -f` returned no matches. Both `pgrep` and `ps` confirmed protected PID **51217** was still running from its `/nix/store/…/Applications/ReShiki.app/Contents/MacOS/reshiki` executable. Cleanup evidence was appended to `process-cleanup.txt`.

The report, screenshot files, and appended process logs were written only within the requested artifact directory.
