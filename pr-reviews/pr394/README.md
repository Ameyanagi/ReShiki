# ReShiki #386 phase 1b: final real-app evidence

- **Branch:** `feat/3d-models-1b` at `9724c826`, worktree `/Users/shiranui/dev/ReShiki/.claude/worktrees/wf_228c0944-39d-1`, clean tree.
- **Binary:** `target/debug/reshiki`, rebuilt from HEAD with `CARGO_INCREMENTAL=0 cargo build --bin reshiki`. The sha256 is in `app/binary-sha256.txt`.
- **Contents:** no `.app` bundles and no target directories. The bundle sits outside this folder.

## How it was made

- **Input:** `input/caffeine.mol` (2D caffeine, 14 atoms and 15 bonds), the same file as the earlier runs.
- **Bundle:** `scratchpad/apps386-final-1b/ReShiki386F1bEvidence.app`, bundle id `dev.reshiki.verification.ev386final1b` (plist in `tools/bundle-Info.plist`). `Contents/MacOS/reshiki` is a **hard link** to the worktree binary (same inode).
- **Launch:** `open -n <bundle> --env RESHIKI_DATA_DIR=scratchpad/ev386-1b-final-data --args --open input/caffeine.mol`. The data dir was new and empty. The PID was 29655 and the window 7293 (`app/launched-process.txt`).
- **Driving:** Codex computer use, `codex exec -m gpt-6-astra -s danger-full-access … < /dev/null`.
  - Prompt: `app/prompt.md`. Codex's report: `app/report.md`. Raw log: `app/codex.log`.
  - Every drag went through the CGEvent helper `tools/drag.swift`, which can capture while the button is held. Exact coordinates are in `app/report.md`.
  - Frame pixel measurements are in `app/frame-measurements.md` and `.json`.
- **Screenshots:** all are window-only captures of that PID (`screencapture -x -o -l 7293`).
- **Cleanup:** Codex quit only PID 29655 (`kill`). `app/process-cleanup.txt` shows it gone and the installed ReShiki (PID 51217) still running and untouched. `app/installed-process-before.txt` records the processes before the run.
- **Morph frames:** `tools/morphcap.sh` captured 40 frames, about 125 ms apart, into `app/morph-raw/`. The overview is `morph-raw/contact-sheet.png`, and Codex's analysis is in `app/morph-analysis.md`.
- **Window and zoom:** the window opened at 1280×852 pt, and the file opened at 238%, where the 3D frame overflows the canvas (`02-initial-238-percent.png`). Codex zoomed out to 122% for the measured sequence.

## Before

**`before-1a/`** holds the phase 1a state, reused from the 1a run:

| File | What it shows |
|---|---|
| `1a-03-selected-2d.png` | Caffeine selected, Structure 2D/3D/Both, and no floating bar. |
| `1a-04-3d.png` | The 3D model in a frame with no resize handles and no bar. |

**`before-1e1d6e15/`** holds the earlier 1b head, before the two fixes in this PR's last commits (`e1bbe65b` bar clamp, `9724c826` fixed frame). It is reused from the previous 1b run:

| File | What it shows |
|---|---|
| `02-3d-shown.png` | Opening 3D view: a square frame. |
| `04-rotate-mid-drag.png`, `05-rotate-released.png` | After a rotation, the frame **refits to the turned model**: tall and narrow, about 440×870 px. In `05`, the bar's lower edge is **clipped under the status bar**. |
| `08-rotated-before-resize.png`, `09-resized.png` | The frame changes aspect with every rotation. |

These were shot in a 1000×665 pt window at 238%, so their scale differs from the `app/` shots.

## After (`app/`, `9724c826`)

I looked at every image.

| File | What it shows |
|---|---|
| `01-selected-2d.png` | Cmd+A on 2D caffeine at 238%. Structure has 2D active. The **Show as** bar sits above the selection, inside the canvas. |
| `morph/m1-f32-generating.png` | 3D clicked: "Generating the conformer · ETKDG, then MMFF94s", and the drawing is still 2D. |
| `morph/m2-f33-morph-start.png` | First ball-and-stick frame at the 2D positions, with no hydrogens yet. |
| `morph/m3-f35-mid-morph-hydrogens-fading-in.png` | Mid-morph: atoms moving, and hydrogens translucent and appearing. |
| `morph/m4-f37-near-end.png` | Near the end. |
| `morph/m5-f39-settled.png` | Settled. f33→f39 is about 790 ms of samples, which fits 750 ms. |
| `02-initial-238-percent.png` | The settled model at the opening zoom of 238%. The frame overflows the canvas top and bottom, and the bar is clamped at the canvas bottom, fully visible. |
| `02-3d-shown.png` | **Baseline** at 122%: a square frame of **744×744 px**. The bar is above it, with 3D active. Structure shows the 2D layout row, with Original drawing active. Status: "Shown as 3D model · Undo restores the 2D drawing". |
| `03-rotate-mid-drag.png` | Captured with the button held. The model is turned edge-on, the **bar is hidden**, and the hint strip reads "Drag to rotate · Shift-drag to roll · Double-click to reset". The frame is **unchanged at 744×744** in the same place. |
| `04-rotate-released.png` | After release: the edge-on view is kept, the **frame is identical to `02`** (744×744, zero offset), and the bar is back above it, fully inside the canvas. Status: "Drawing updated". Compare `before-1e1d6e15/05-rotate-released.png`. |
| `05-zoomed-bar-clamped.png` | Zoomed to 238% with the turned model selected. The frame is taller than the canvas, and the **bar is clamped inside the canvas bottom**, about 8 px above the status bar, with all three buttons visible. |
| `05b-zoom-restored.png` | Back at 122%, the same as `04`. |
| `06a-shift-roll-mid-drag.png` | Shift-drag with the button held: the hint shows and the bar is hidden. |
| `06-shift-roll.png` | After the Shift-drag, the edge-on model is rolled in the screen plane from vertical to diagonal. The frame is still 744×744. |
| `07-double-click-reset.png` | After a double-click, the opening orientation matches `02`, with the same frame. |
| `08a-resize-mid-drag.png`, `08-resized.png` | Bottom-right corner drag: the frame becomes **884×884** with the top-left fixed, and the model is scaled up. |
| `09a-resized-rotate-mid-drag.png`, `09-resized-then-rotated.png` | Rotated after the resize: an oblique view, and the **frame stays 884×884** in the same place, so the resized size is kept. |
| `10-layout-from-3d.png` | 2D layout → **From the 3D view** is active: "Flatten the model as it is turned now, with far bonds lighter." |
| `11-flattened-2d.png` | After 2D: a flattened projection of the turned view, with far bonds and labels lighter and 2D active. Status: "Shown as 2D from the 3D view · Undo restores the 3D model". |
| `12-back-to-3d.png` | 3D again: the turned model is back at 884×884, and From the 3D view is still chosen. The frame has moved up and left; see problem 2. |
| `13-layout-original.png` | 2D layout → **Original drawing** is active: "The layout you drew, unchanged." |
| `14-original-restored.png` | After 2D: the **original caffeine layout** from `01` is back, with the same ring orientation and substituent directions. Codex's 14-atom fit gives 0.13 px RMS for scale plus translation only. Status: **"Original 2D drawing restored"**. |
| `15-undo.png` | One Cmd+Z: the turned 3D model is back with Original drawing active. Redo is enabled. Status: "History restored". |
| `16-context-menu.png` | Right-click → **Show as** submenu: 2D drawing / 2D from the 3D view / 3D model (disabled, current) / 2D and 3D. It was opened by click, because the computer-use API has no hover. |
| `17a-both-at-122-percent.png` | **Both** at 122%: the 2D drawing and the 3D model side by side. The frame and model run under the inspector. |
| `17-both.png` | Both at 97%: the 2D drawing on the left and the 3D model on the right. Both is active in the bar and in Structure. Status: "Shown as 2D and 3D · Undo restores the 3D model". |
| `18-both-deselected.png` | Deselected: both representations stay. Properties shows the whole-drawing panel. |

## Problems and things worth a look (not fixed)

1. **Both shows the flattened 2D while "Original drawing" is selected** (`15`, `17`, `18`).
   - **Sequence:** after `14` (original restored) and one Undo (`15`), the 2D layout row says **Original drawing · "The layout you drew, unchanged."**
   - **What happens:** choosing Both draws the **flattened projection** from step 11 as the 2D member, not the drawn layout.
   - **Likely cause:** Undo brought back a state in which the stored 2D coordinates were still the flattened ones.
   - **Why it matters:** the label and the 2D drawing disagree.
   - The Both-mode 2D member is also drawn very small next to the 3D model.
2. **The 3D frame moves when you go 2D-from-3D → 3D** (`11` → `12`).
   - **Size:** kept at 884×884.
   - **Position:** it shifts about 43 pt left and 40 pt up, apparently re-centred on the smaller flattened drawing.
   - **Scope:** this is small, and it is not the rotation fix, which holds.
3. **Both at 122% overflows under the inspector** (`17a`). The joint frame and the model's right side are hidden behind the Properties panel until you zoom out, and the canvas does not scroll it into view. The bar itself stays inside the canvas.
4. **The flattened 2D is hard to read in steep views** (`11`), the same as before. Foreshortened bonds become stubs and labels overlap. This is a projection by design.
5. **The Structure note says "Ball & stick · Chemistry and atom IDs stay the same" even while only 2D is shown** (`01`, `11`, `14`). This is cosmetic.
6. **Known, not phase-1b issues:**
   - The keyboard hotspot crosshair lands on the model (`07`, `09`, `11`).
   - The active segmented button in Structure renders as pale grey text, which reads like a disabled button.
7. **No problems seen** with the two fixes under test:
   - The frame stayed exactly 744×744, or 884×884 after the resize, through rotate, roll, reset and resize-then-rotate.
   - The bar was fully inside the canvas in every shot, including the zoomed-in `05` and `02-initial-238-percent`.

## Not verified

- One Undo step per rotation (only the restore undo was tested).
- Hover opening of the context submenu.
- Save and reopen.
- Rotation fallbacks: an unselected model, a drag near the edge, and Alt or Cmd drags.
- Exact morph timing (about 125 ms sampling).

## Fix check (`app-fixes/`, `a50509bf`)

- **What changed:** two fixes landed after the run above.
  - `2cd64d2e`: the 3D frame keeps its place through 2D and back (problem 2).
  - `a50509bf`: choosing Original drawing restores the drawn layout in the same Undo step (problem 1).
- **Binary:** `feat/3d-models-1b` at `a50509bf`, clean tree, built with `CARGO_INCREMENTAL=0 cargo build --bin reshiki`. The sha256 is in `app-fixes/binary-sha256.txt`.
- **Bundle:** `scratchpad/apps-appfix-1b/ReShiki386Fix1bEvidence.app`, bundle id `dev.reshiki.verification.ev386fix1b` (`app-fixes/tools/bundle-Info.plist`). `Contents/MacOS/reshiki` is a hard link to the worktree binary.
- **Launch:** `open -n … --env RESHIKI_DATA_DIR=scratchpad/ev386-1b-appfix-data --args --open input/caffeine.mol`. The data dir was new. PID 76822, window 7636, 1280×852 pt.
- **Driving:** Codex computer use with `codex exec -m gpt-6-astra -s danger-full-access … < /dev/null`.
  - Prompt: `app-fixes/prompt.md`. Report: `app-fixes/report.md`. Measurements: `app-fixes/frame-measurements.md`.
  - All shots are window-only (`screencapture -l 7636`).
- **Cleanup:** Codex killed only PID 76822. The installed PID 51217 was untouched (`app-fixes/process-cleanup.txt`).
- **Zoom:** 122% for the 3D and 2D steps (as old `09`–`15`) and 97% for Both (as old `17`).
- **Setup:** the drawing opened in keyboard drawing mode, so the first resize drag moved the keyboard cursor (`A02-resize-first-attempt.png`). F8 left that mode, and the same drag then resized the frame.

I looked at every image. The frame edges were re-measured independently: long runs of the exact frame colour RGB (62,133,117).

| File | What it shows |
|---|---|
| `A01-3d-122.png` | 3D at 122%. The frame is 744×744 px at (664,603)–(1408,1347), the same as old `02`. |
| `A02-resized.png` | After a corner drag: 884×884 px, with the top-left fixed. |
| `A03-resized-rotated.png` | Rotated. The frame is still 884×884 px at (664,603)–(1548,1487). **This is the BEFORE frame.** |
| `A04-layout-from-3d.png` | 2D layout → From the 3D view is active. The frame is unchanged. |
| `A05-flattened-2d.png` | 2D: the flattened projection. Status: "Shown as 2D from the 3D view · Undo restores the 3D model". |
| `A06-back-to-3d.png` | **Fix 1.** 3D again. The frame is at (664,603)–(1548,1487) and 884×884 px, so **every edge moved 0 px** from `A03`. Before the fix, old `11`→`12` moved the frame from (664,603) to (585,531): −79/−72 px, about 40/36 pt up and left. |
| `A07-layout-original.png` | **Fix 2.** Original drawing is chosen and the model stays 3D. Status: **"Original 2D drawing restored"** at once, without needing 2D. The frame is still 0 px off. |
| `A08-original-2d.png` | 2D: the drawn caffeine, upright fused 6+5 rings, the same as the opening drawing (`A00`). Status: "Shown as 2D · Undo restores the 3D model". |
| `A09-undo.png` | One Cmd+Z: 3D, with **Original drawing** active and Redo enabled. Status: "History restored". Compare old `15`. |
| `A10-both-122.png`, `A11-both-97.png` | **Fix 2.** Both: the 2D member is the **drawn caffeine**, not the projection from `A05`. The layout row says **Original drawing**. Status: "Shown as 2D and 3D · Undo restores the 3D model". Compare old `17`, which drew the projection. |
| `A12-both-deselected.png` | Deselected at 97%: the drawn 2D and the turned 3D sit side by side. |
| `A13-undo-both.png` → `A14-redo-both.png` | Undo Both (3D, Original drawing), then Redo: Both again, still the drawn caffeine, with Original drawing active. |
| `A15a-undo-to-3d-original.png` → `A15b-undo-original-restore.png` → `A15-redo-2d.png` → `A16-both-after-redo.png` | Undo the Both, then undo the Original drawing choice: From the 3D view is back. Redo brings back **Original drawing**, with 3D kept. Then Both: the drawn caffeine again. |

The flow was "Undo; Both; Redo; Both again". Choosing Both after the Undo replaces the redo entry for the 2D switch, so Redo after Undo-then-Both can only redo Both (`A13`→`A14`). `A15b`→`A16` covers the redo that matters here: redoing the Original drawing choice, then Both.

**Things worth a look (not regressions from these fixes):**

- At 122% and 97%, the Both frame still runs under the inspector on the right (`A10`, `A11`, `A14`), as in old `17`/`17a` (problem 3 above).
- This build has no native Edit menu, so Redo was checked with the toolbar button and Cmd+Shift+Z.
- The keyboard hotspot crosshair shows on the model in `A01` (known).
