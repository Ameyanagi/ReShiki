# ReShiki #386 phase 2: evidence

- **Branch:** `feat/3d-models-2` at `588b000b`
- **Worktree:** `/Users/shiranui/dev/ReShiki/.claude/worktrees/wf_228c0944-39d-2`
- **Binary:** `target/debug/reshiki`, built 18:16, after the last commit (18:13). The working tree was clean. The sha256 is in `app/binary-sha256.txt`. I did not rebuild it.
- **Contents:** no build target directories and no .app bundles. The evidence bundle and the Chrome profile are kept outside this folder, in `scratchpad/apps386-2/` and `scratchpad/ev386-2-chrome/`.

## 1. Renders through the repo CLI (`renders/`, inputs in `input/`)

**Inputs** (`tools/make_inputs.py`):
- `input/<mol>-base.rsk` are the phase-2 evidence files from `scratchpad/p2/ev/`. They hold caffeine and ibuprofen with ReShiki's own MMFF94s conformer, made by the branch's geometry worker.
- For each molecule, the script writes one file per style × look, 4 × 2 = 8:
  - `display = three_d`, `offset = 0`
  - `style`: ball_stick, stick, wireframe or space_filling
  - `look`: flat or glossy
- `input/own/`: ReShiki's own conformer and opening view.
- `input/proto/`: the same drawing with the model coordinates replaced by the prototype's (`model3d/mols.json`, RDKit MMFF94s, seed 7). The rotation is set to ReShiki's `initial_rotation` of those coordinates.
  - The Python port reproduces the quaternions stored by Rust to 6 dp: caffeine `[0.118239, 0.960246, -0.143464, -0.208245]`, ibuprofen `[0.01006, 0.973761, -0.008093, -0.227207]`.
- `input/proto-matched/` (ibuprofen only): see section 2.

**Rendering** (`tools/render_all.sh`, using the branch binary):
- `reshiki --cli convert X.rsk -o X.svg` writes the vector export.
- `reshiki --cli convert X.rsk -o X-export-1200dpi.png` is the **enlarged** image. It is the PNG export path at 1200 dpi, about 5.7× the drawing scale. Sizes are in `convert.log`.
- `reshiki --cli render X.rsk -o X-100pct.png --width <viewBox width>` is the **100%** image, at one pixel per drawing unit. That is the canvas at 100% on a 1× screen, about 235 px wide for caffeine.

**Contact sheets** (rows are styles, columns are Flat and Glossy):
- `renders/sheet-caffeine-export.png`, `renders/sheet-ibuprofen-export.png`: enlarged.
- `renders/sheet-caffeine-100pct.png`, `renders/sheet-ibuprofen-100pct.png`: 100%.

**What they show:**
- **Ball & stick:** shaded balls and two-colour sticks. Flat has thin dark outlines; Glossy has a brighter highlight and darker rim with no outlines.
- **Stick:** cylinders join at small rounded caps, with the colour change at mid-bond.
- **Wireframe:** atom-coloured lines; Flat and Glossy are identical (see problems).
- **Space-filling:** van der Waals spheres.
- All 16 images per molecule rendered without errors.

**Close-up:** `renders/crops/caffeine-stick-flat-seam-crop-x3.png` is a 3× nearest-neighbour crop of the 1200-dpi Stick export. See problem 1.

## 2. Against the prototype (`compare/`)

**Prototype file:**
- `compare/prototype-artifact-89rRsm9QDX5gwanE7kokSm.html` is the approved prototype artifact, read through the Artifact tool and unedited.

**Browser capture:**
- Codex computer use captured it (`codex exec -m gpt-6-astra -s danger-full-access`, prompt `compare/proto-prompt.md`, log `compare/proto-codex.log`, report `compare/proto-shots/report.md`).
- Chrome ran as a separate instance with a throwaway profile. Its PID was 70420 and its window 7128.
- Codex set the browser zoom to 75% so the canvas and inspector fit.
- The view was 3D only, with defaults otherwise: Hydrogens All, Depth 35%, Perspective 20%, Background None. The model was never dragged, so it shows the opening view.

**Captures:**
- `compare/proto-shots/<mol>-<style>-<look>.png`: 16 window-only captures.
- `compare/proto-shots/<mol>-strip-<look>.png`: the prototype's own four-style strip.
- Codex left some setup shots, `setup*.png`. They are not evidence.

**Side by side** (`tools/side_by_side.py`):
- Left is the prototype's model canvas, cropped from the screenshot. Right is ReShiki's 1200-dpi export of the **same coordinates**. Both are trimmed to ink and scaled to the same height.
- Per style: `compare/sbs-<mol>-<style>-<look>.png`.
- Per molecule and look: `compare/sheet-caffeine-flat.png`, `sheet-caffeine-glossy.png`, `sheet-ibuprofen-flat.png` and `sheet-ibuprofen-glossy.png`.
- `compare/sheet-proto-matched-ibuprofen-{flat,glossy}.png` and `sbs-proto-matched-*`: ibuprofen with the rotation set so ReShiki's screen x/y projection equals the prototype's (`tools/proto_view.js` runs the prototype's `initialView` verbatim; `tools/make_matched.py`).
  - This is the prototype's matrix with its z row negated, the closest proper rotation.
  - Depth order is reversed relative to the prototype, so which atom covers which can differ.

**Reading:**
- **Caffeine:** each style and look matches the prototype closely in geometry, sizes, colours, gradients, outline rules and the wire width.
- **Ball-and-stick ordering:** the prototype sometimes draws a bond over its own front ball, for example on the top methyl. ReShiki's ball covers its half-bond. This is the later ordering fix, not a regression.
- **Ibuprofen in the matched view:** styles and looks match the prototype in the same way.
- **Ibuprofen opening view:** differs from the prototype. See problem 3.

## 3. Real app (`app/`)

**Bundle:**
- Path: `scratchpad/apps386-2/ReShiki386P2Evidence.app`. The bundle id is `dev.reshiki.verification.ev3862p2`, and its plist is copied to `tools/bundle-Info.plist`.
- `Contents/MacOS/reshiki` is a **symlink** to the worktree binary, and it launched fine this time.
- Codex launched it with `open -n <bundle> --env RESHIKI_DATA_DIR=scratchpad/ev386-2-data --args --open input/own/caffeine-ballstick-flat.rsk`.
- PID 74366, window 7185 (`app/launched-process.txt`, `app/window-verification.txt`).
- Codex attached by bundle id and quit with `kill 74366`. `app/process-cleanup.txt` shows it gone, with the installed ReShiki (PID 51217) still running.

**Driving:**
- Codex computer use with `gpt-6-astra`. The prompt is `app/prompt.md`, the report `app/report.md` and the log `app/codex.log`.
- The slider drag used the CGEvent drag helper (`tools/drag.swift`).
- Zoom used `+` and then Cmd+scroll (`app/cmd-scroll.swift`).
- Evidence shots are `screencapture -l 7185`, window only.

**Preflight:**
- `app/00-preflight-launch.png` is my own launch test of the symlinked bundle (PID 69547, killed by PID). It shows the file opening as a 3D ball-and-stick model at 201%.

**Screenshots** (I looked at every one):

| File | What it shows |
|---|---|
| `01-section-ballstick.png` | The model is selected. Properties shows Structure (3D active) and the **3D model** section: four Style buttons with pictograms (Ball & stick active), Look Flat · journal / Glossy · slides, Hydrogens All/Polar/None, Depth fade 35%, Perspective 20%, Background None/Soft card and Reset view. Atom labels and Regenerate are absent, as intended. |
| `02-style-stick.png` | Stick is active and the section subtitle says "Stick". The model is drawn as cylinders. |
| `03-style-wireframe.png` | Wireframe is active, with thin atom-coloured lines. |
| `04-style-spacefilling.png` | Space-filling is active, with van der Waals spheres. The frame is somewhat larger than for Ball & stick: 823 vs 777 display px wide. |
| `05-style-ballstick-again.png` | Back to Ball & stick, with the same frame as `01`. |
| `06-look-glossy.png` | Glossy is active. The balls have strong highlights and no outlines (see `crops/looks-05flat-06glossy-09undo-08undo.png`). |
| `07-depth-fade-dragged.png` | After one drag, Depth fade reads 87% and the whole model is paler. Caffeine is nearly flat, so the front-to-back difference is small. |
| `07b-edit-menu-full-screen.png` | **Full screen.** The app has no native Edit menu; its menu bar shows only the app name. This shot also shows other windows on the desktop (a browser) and stacked Finder alerts saying "The application 'ReShiki' is not open anymore", which are leftovers from killed test instances. Do not share it as is. |
| `08-undo-slider.png` | One ⌘Z: Depth fade is back to 35%, Flat and Ball & stick are unchanged, and the status reads "History restored". |
| `09-undo-again.png` | A second ⌘Z undoes the earlier Flat click, so Glossy returns and Depth fade stays at 35%. With `08`, this shows that **one slider drag is one Undo step**. |
| `10-zoom-300-ballstick.png` | Deselected at exactly **300%**, Ball & stick, Flat. The balls have smooth radial shading with **no stepped rings or banding**. 1:1 crop: `crops/10-zoom-300-crop-1to1.png`. |
| `11-zoom-400-ballstick.png` | At 400%, the same result: smooth and sharp, with no banding. |

The status bar reads "Drawing updated" after each change and "History restored" after Undo and Redo.

## Problems seen (not fixed)

1. **A hairline seam crosses same-coloured bonds at their middle.**
   - **Where:** a thin pale line across C–C (and other same-element) sticks where the two half-bonds meet.
   - **In the export:** visible in the 1200-dpi PNG export (`renders/crops/caffeine-stick-flat-seam-crop-x3.png`).
   - **On the canvas:** also visible at 300% and 400% (`app/crops/10-zoom-300-crop-1to1.png`; Codex saw it independently).
   - **Not a shading jump:** the gradient runs on correctly across the seam. It looks like anti-aliasing where two separately filled trapezoids abut (conflation).
   - **Severity:** small at 100%, but visible in enlarged figures. Overlapping the halves slightly, or filling a same-colour bond as one shape, would probably remove it.
2. **The Structure section subtitle goes stale.**
   - The Structure card keeps saying "**Ball & stick** · Chemistry and atom IDs stay the same" after Style is changed to Stick, Wireframe or Space-filling (`02`–`04`).
   - The 3D model section subtitle updates correctly.
3. **Ibuprofen opens in a different view from the prototype.**
   - **What happens:** ReShiki shows the aromatic ring nearly face-on; the prototype shows it edge-on (`compare/sheet-ibuprofen-flat.png`).
   - **Cause, verified numerically:**
     - Both use the same principal axes, so the pre-tilt bases are identical except the z row.
     - ReShiki's z row is negated deliberately, to turn the prototype's reflection into a proper rotation.
     - The fixed tilt `rotX(-0.35)·rotY(0.3)` then mixes the opposite z into screen x/y.
     - For a flat molecule like caffeine this is invisible. For ibuprofen the second and third principal spreads are close (10.3 vs 9.4 Å²), and the screen-y axis turns about 40°.
   - **Is it wrong?** It is not wrong as such. The prototype's ibuprofen is a mirror image, the wrong hand for (S)-ibuprofen. But "same opening view as the prototype" holds only for near-planar molecules. Flipping the tilt signs together with the z fix would keep the prototype's look for 3D-shaped molecules.
4. **Wireframe looks identical in Flat and Glossy.** This matches the prototype, where Look changes only ball and stick gradients and outlines. It may still surprise users, because the Look buttons do nothing in Wireframe.
5. **Space-filling makes the selection frame a little larger** than Ball & stick, about 6% wider (`04` vs `01`). This is consistent with "keeps the frame about the same size", so it is not a bug.
6. **Known, not phase-2 issues:**
   - The keyboard hotspot crosshair sits on the model.
   - The app has no native Edit menu, so the Undo label could not be checked there. The toolbar's accessible label is generic: "Undo · ⌘Z".

## Not verified

- Hydrogens Polar/None, Perspective, Soft card and Reset view were seen in the panel but not exercised in the app. Their unit tests exist.
- A 1× (non-Retina) display. The canvas renders at 2×, so a 1× display would be downsampled.
