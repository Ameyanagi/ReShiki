# ReShiki #386 phase 1b: real-app evidence

The branch is `feat/3d-models-1b` at `1e1d6e15`, in the worktree
`/Users/shiranui/dev/ReShiki/.claude/worktrees/wf_228c0944-39d-1`. The binary
is `target/debug/reshiki`, built from HEAD with `CARGO_INCREMENTAL=0 cargo build
--bin reshiki`. Its sha256 and the commit are in `app/binary-sha256.txt`.

"Before" means the phase 1a branch build, not `origin/main`. Its screenshots
are reused from the 1a evidence run, in `before-1a/`.

## How it was made

1. **Input.** `input/caffeine.mol` comes from the branch CLI:
   `reshiki --cli convert --smiles 'CN1C=NC2=C1C(=O)N(C(=O)N2C)C' -o caffeine.mol`.
   It is a 2D layout with 14 atoms and 15 bonds.
2. **App bundle.** The bundle is `scratchpad/apps1b2/ReShiki1bEvidence.app`,
   which sits outside this folder. Its bundle id is
   `dev.reshiki.verification.ev3861b2`, and its Info.plist is copied to
   `tools/bundle-Info.plist`. `Contents/MacOS/reshiki` is a **hard link** to the
   worktree binary, not a symlink. Two things forced that:
   - With the symlink, LaunchServices marked the bundle `no-executable`.
   - During this run the worktree's whole `target/` directory was deleted from
     outside this task, and I had to rebuild.

   A hard link is the same inode as the binary (checked with `ls -li`), and it
   survives another deletion.
3. **Launch.** Codex ran
   `open -n <bundle> --env RESHIKI_DATA_DIR=scratchpad/ev386-1b-data --args --open input/caffeine.mol`.
   The data dir is separate, so recovery drafts and settings don't touch the
   installed app. The PID was 46018 and the window 7018 (see
   `app/launched-process.txt`).
   - Codex attached computer use by bundle id and quit only that PID, with
     `kill 46018`. `app/process-cleanup.txt` shows the PID gone and the
     installed ReShiki (PID 51217, from /nix/store) still running.
     `app/installed-process-before.txt` records the installed app before the run.
4. **Driving.** Codex computer use (`codex exec -m gpt-6-astra -s
   danger-full-access`) drove the app. The prompt is `app/prompt.md`, Codex's
   report is `app/report.md`, and the raw log is `app/codex.log`.
   - The computer-use API cannot hold the mouse button while doing something
     else. So every drag went through `tools/drag.swift` (compiled to
     `scratchpad/ev386-1b-drag`). It posts real CGEvent mouse-down, 12 drag
     steps and mouse-up in window-relative points, with optional Shift. It can
     run `screencapture` while the button is still down.
   - The exact drag coordinates are in `app/report.md`.
5. **Screenshots.** Every evidence shot was `screencapture -x -o -l <window id>`,
   which captures only that PID's window.
6. **Morph frames.** `tools/morphcap.sh` ran in the background, waiting for
   `app/morph-go`. Codex wrote the PID into that file right before clicking 3D.
   - The script then captured the window 40 times, about every 130 ms, into
     `app/morph-raw/` (log: `morph-raw/log.txt`).
   - Frames f01–f26 are identical, as are f33–f40. The changing frames are
     copied, with names, into `app/morph/`.
7. **Earlier failed attempt.** `app/run1-blocked/` holds a first Codex run that
   could not attach to a bundle I had launched myself. LaunchServices had
   flagged that bundle `no-executable`. Nothing was driven in that run.

## Evidence files (my reading of each image)

| File | What it shows |
|---|---|
| `before-1a/1a-03-selected-2d.png` | 1a: caffeine selected, Structure section with 2D/3D/Both. There is no floating bar. |
| `before-1a/1a-04-3d.png` | 1a: the 3D model in a frame with no resize handles and no bar. |
| `app/01-selected-2d.png` | 1b: Cmd+A on 2D caffeine. The Structure section has 2D active, and the floating **Show as** bar is above the selection even though no model exists yet. |
| `app/morph/m1-f27-generating.png` | Generating. Status: "Generating 3D model · MMFF94s". The bar and Structure section already show 3D as pending, and the drawing is still 2D. |
| `app/morph/m2-f28-morph-start.png` | First morph frame: balls and sticks at the 2D drawing positions, with no hydrogens yet. |
| `app/morph/m4-f30-morph-hydrogens-fading-in.png` | Mid-morph: heavy atoms partly moved, and hydrogens translucent and growing out of their carbons. |
| `app/morph/m6-f32-morph-near-end.png` | Near the end: hydrogens almost opaque. |
| `app/morph/m7-f33-settled.png` = `app/02-3d-shown.png` | Settled 3D model. The change took 6 frames (~0.54–0.81 s), which fits 750 ms. |
| `app/03-bar.png` | The bar sits **below** the frame because the frame reaches the top of the canvas. 3D is active. Status: "Shown as 3D model · Undo restores the 2D drawing". |
| `app/04-rotate-mid-drag.png` | Captured while the button was held: the model is turned nearly edge-on, the bar is hidden, and a hint strip says "Drag to rotate · Shift-drag to roll · Double-click to reset". |
| `app/05-rotate-released.png` | After release: the orientation is kept and the bar is back. Its lower edge is clipped by the canvas bottom (see problems). Status: "Drawing updated". |
| `app/06-shift-roll.png` | After a Shift-drag: the same edge-on view, rolled in the screen plane from vertical to diagonal. |
| `app/07-double-click-reset.png` | After a double-click: the opening orientation and frame, matching `02`. |
| `app/08-rotated-before-resize.png` | A second plain rotation, in a different direction. |
| `app/09-resized.png` | After a bottom-right corner drag of +100/+80 pt: frame and model scaled with the top-left fixed. Corner handles only, no edge handles. |
| `app/10-layout-from-3d.png` | Structure → 2D layout → **From the 3D view** active: "Flatten the model as it is turned now, with far bonds lighter." |
| `app/11-flattened-2d.png` | After 2D: a 2D drawing in the projection of the turned view (not the original layout). Far bonds and labels are lighter, and the bar shows 2D. Status: "Shown as 2D from the 3D view · Undo restores the 3D model". |
| `app/12-undo.png` | One Cmd+Z: the resized, turned 3D model is back, From the 3D view is still chosen, and Redo is enabled. Status: "History restored". |
| `app/13-context-menu.png` | Right-click → Show as: 2D drawing / **2D from the 3D view** / 3D model (disabled, current) / 2D and 3D. |

## Things that looked wrong or worth a look

1. **The bar is clipped at the canvas bottom** (`05`). It falls back to below the
   selection, but there is no room there either, and its lower border runs
   under the status bar. The buttons stay readable and clickable. The placement
   logic does not clamp to the visible canvas.
2. **The frame resizes on every rotation.** Its size and aspect follow the
   rotated model's projected bounds:
   - `02`: square, about 915×830 px.
   - `04`/`05`: tall and narrow, 440×870 px.
   - `06`: about 760×705 px.
   - `08`: wide.

   So the selection box, handles and bar jump around while you turn the model,
   and a frame size set by a corner resize is probably not kept through a later
   rotation. I did not test resize-then-rotate. This may be intended, since the
   frame is refitted to the model, but it is not a fixed frame.
3. **The bar also shows for plain 2D molecules with no model** (`01` and
   `morph/m0`). Every whole-molecule Select-all now puts a floating Show as bar
   over ordinary 2D drawings. This matches "bar for a whole selected molecule",
   but it is the deviation 1 trade-off made visible on every 2D selection.
4. **The flattened 2D drawing is hard to read in steep views** (`11`).
   Foreshortened bonds become stubs: the lower C=O is a tiny double tick under
   its carbon, and the N–CH3 on the right is very short. The imidazole C–H
   corner becomes a sharp ">". This is what a projection does, but the result is
   poor as a 2D figure.
5. **The keyboard hotspot crosshair sits on top of the model.** It lands on a
   hydrogen in `02`/`07`, and on a ring atom in `08`. This was a known 1a
   limitation and is still visible.
6. **Molecular properties briefly loses the formula** during the morph
   (`morph/m2`–`m4` say "Selection" only), and it comes back once the morph
   settles. This is cosmetic.
7. **Not app problems:**
   - The "Recovery draft · Restore · Dismiss" footer in every shot comes from a
     first instance I launched and then SIGTERMed (in the isolated data dir).
   - The 2D layout row disappears once 2D is shown (`11`). That is the
     documented "while the model is shown" behaviour.

## Not verified

- One Undo step per rotation (only the flatten undo was tested).
- Rotation fallbacks: an unselected model, a drag within 8 px of the edge, and
  Alt or Cmd drags.
- Resize-then-rotate.
- Both mode.
- "Original drawing" restore and the save/reopen round trip.
- Hover opening of the context submenu (it was opened by click).
- Precise morph duration and smoothness (~130 ms sampling).
