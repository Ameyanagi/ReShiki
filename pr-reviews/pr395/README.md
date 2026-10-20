# ReShiki #386 phase 2: final evidence

- **Branch:** `feat/3d-models-2` at `5838e6c6` (includes 1b, merged in `7d12ad3e`), worktree `/Users/shiranui/dev/ReShiki/.claude/worktrees/wf_228c0944-39d-2`, clean tree.
- **Binary:** `target/debug/reshiki`, rebuilt from HEAD with `CARGO_INCREMENTAL=0`. The sha256 is in `app/binary-sha256.txt`.
- **Fixes since the previous evidence run (`588b000b`):**
  - `fd737dcf`: no pale seam where a stick's two halves meet.
  - `5838e6c6`: the Structure note names the current style.
  - The 1b frame and bar fixes, via the merge.
- **Contents:** no `.app` bundles and no target directories. The bundle is `scratchpad/apps386-final-2/`, outside this folder.

## Before (`before-588b000b/`, from the previous phase-2 run)

| File | What it shows |
|---|---|
| `02-style-stick.png`, `03-style-wireframe.png` | The Structure note is stale: it still says "**Ball & stick** · Chemistry and atom IDs stay the same" with Stick or Wireframe active. |
| `10-zoom-300-ballstick.png`, `10-zoom-300-crop-1to1.png`, `11-zoom-400-ballstick.png` | The app at 300% and 400%: a thin pale line crosses bonds where the two halves meet. |
| `caffeine-stick-flat-seam-crop-x3.png` | The same seam in the 1200 dpi CLI export (3× crop). |

## 1. CLI renders (`renders/`, regenerated from `5838e6c6`)

**Inputs and tools:**
- The inputs (`input/`) and tools (`tools/`) are unchanged from the previous run.
- `tools/render_all.sh` writes, per file, `X.svg`, `X-export-1200dpi.png` (the PNG export path) and `X-100pct.png` (`--cli render` at one pixel per drawing unit). The log is `renders/render.log`.
- `own/` is ReShiki's MMFF94s conformer at its opening view; `proto/` uses the prototype's coordinates.

**Contact sheets** (rows: Ball & stick, Stick, Wireframe, Space-filling; columns: Flat, Glossy):
- `sheet-caffeine-export.png`, `sheet-ibuprofen-export.png`: 1200 dpi.
- `sheet-caffeine-100pct.png`, `sheet-ibuprofen-100pct.png`: 100%.

All 16 renders per molecule render cleanly. Flat has thin dark outlines and Glossy has stronger highlights. As before, Wireframe is identical in Flat and Glossy.

**Stick midpoint at 1200 dpi** (`renders/crops/`):

| File | What it shows |
|---|---|
| `caffeine-stick-flat-midpoint-before-after-x3.png` | The same C–C region, before (`588b000b`) and after (`5838e6c6`), 3× nearest-neighbour. **Before:** a pale line across both C–C sticks. **After:** none. In this crop, 174 pixels are ≥8 levels lighter in the old render, up to 34 levels; they are the seam. |
| `caffeine-stick-flat-midpoint-crop-1to1.png`, `-x3.png` | The after crop alone. |
| `caffeine-ballstick-flat-cn-bond-before-after-x3.png` | A Ball & stick Flat C–N bond: the pale line at the colour boundary is gone. |
| `midpoint-crops-other-styles-x3.png` | Stick Glossy and Ball & stick Flat and Glossy near the same junction: no seam. The 4th tile (ibuprofen) misses a midpoint and can be ignored. |

**Prototype comparison** (`compare/`):
- `proto-shots/` and the prototype HTML are reused unchanged, because the prototype has not changed.
- The `sbs-*` and `sheet-*` side-by-sides are **regenerated** against the new exports (`tools/side_by_side.py`). The match is as in the previous README: caffeine matches; ibuprofen matches only in the `proto-matched` view.

## 2. Real app (`app/`)

**Bundle:** `ReShiki386F2Evidence.app`, bundle id `dev.reshiki.verification.ev386final2` (`tools/bundle-Info.plist`). `Contents/MacOS/reshiki` is a **hard link** to the branch binary.

**Launches:** four, one after another, each with `open -n … --env RESHIKI_DATA_DIR=… --args --open …`.

| Run | File | Data dir | PID | Window |
|---|---|---|---|---|
| A | caffeine | `ev386-2-final-data` | 38356 | 7402 |
| B | ibuprofen | `ev386-2-final-data` | 40976 | 7540 |
| C | caffeine Stick | `ev386-2-final-data-dark`, which holds `appearance.json` = `{"mode":"dark"}` | 41441 | 7579 |
| C′ | as C, for the dark canvas | as C | 46469 | 7602 |

- **Driving:** Codex computer use, `codex exec -m gpt-6-astra -s danger-full-access … < /dev/null`.
  - Prompts: `app/prompt.md` and `app/prompt-dark-canvas.md`.
  - Reports: `app/report.md` and `app/report-dark-canvas.md`. Logs: `codex*.log`.
  - Drags used the CGEvent helper; the commands are in `app/drag-commands.txt`.
- **Screenshots:** all are window-only, `screencapture -l <wid>`.
- **Cleanup:** each run was quit with `kill <its PID>` only. `app/process-cleanup.txt` shows each PID gone and the installed ReShiki (PID 51217) untouched.

**Window and zoom:** the window is 1280×852 pt. Caffeine opens at 158% and ibuprofen at 121%.

I looked at every image.

### Run A: caffeine

| File | What it shows |
|---|---|
| `A01-section-ballstick.png` | The model is selected, and the **3D model** section is visible: Style with pictograms, Look, Hydrogens, Depth fade 35% and Perspective 20%. The Structure note reads "**Ball & stick** · Chemistry and atom IDs stay the same", and the 3D model subtitle reads "Ball & stick". |
| `A02-style-stick.png` | Stick. The Structure note now reads "**Stick** · …", and the subtitle "Stick". Compare `before-588b000b/02`. |
| `A03-style-wireframe.png` | Wireframe, with the note "**Wireframe** · …". |
| `A04-style-spacefilling.png` | Space-filling, with the note "**Space-filling** · …". |
| `A05-style-ballstick.png` | Back to Ball & stick, with the note "Ball & stick · …". |
| `A06-look-glossy.png` | Glossy · slides is active: stronger highlights and no dark outlines. |
| `A07-hydrogens-polar.png` | Polar: every hydrogen is hidden, which is correct because caffeine has no O–H or N–H. The model refits in the same frame. |
| `A08-hydrogens-none.png` | None: the same image as Polar, with None active. |
| `A09-perspective-100.png` | One drag to **Perspective 100%**: strong foreshortening, so the near methyl H atoms are large. The selection frame is **not drawn** and the bar has moved to the canvas bottom; see problem 1. |
| `A09b-perspective-undo-20.png` | One Cmd+Z: Perspective is back to 20%, with the frame and bar back. Status: "History restored". |
| `A10-soft-card.png` | Background Soft card: a pale rounded card behind the model, inside the frame. Properties is scrolled to show Background and Reset view. |
| `A11-rotated.png` | A drag rotation to an edge-on view. The frame is unchanged. |
| `A12-reset-view.png` | **Reset view**: the opening orientation, the same as `A05`. The unsaved dot clears because the document matches the file again. |
| `A13-stick-rotate-mid-drag.png` | Stick, captured with the button held: the bar is hidden and the hint strip reads "Drag to rotate · Shift-drag to roll · Double-click to reset". |
| `A14-stick-rotate-released.png` | Released: the bar is back and the frame is unchanged. |
| `A15-zoom-400-stick.png` | **400%** (set through the Commands palette `zoom 400`), Stick, deselected. The ring-fusion C–C stick is at the centre. |
| `crops/A15-cc-midpoint-1to1.png` | A 1:1 crop (400×400 device px) centred on that C–C midpoint: **no pale seam**, and the shading runs straight through. `A15-cc-midpoint-analysis-crop.png` is Codex's own crop. Compare `before-588b000b/10-zoom-300-crop-1to1.png`. |
| `A16-zoom-400-ballstick.png`, `crops/A16-cc-midpoint-1to1.png` | Ball & stick Flat at 400%: no seam on the C–C stick. |
| `crops/A16-cn-bond-before-after-x3.png` | The same C–N bond at 400%. **Before:** a pale line at the grey/blue boundary. **After:** gone. See problem 2 for a small side effect. |

### Run B: ibuprofen

| File | What it shows |
|---|---|
| `B01-ballstick.png` | The note reads "Ball & stick · …". |
| `B02-stick.png` | The note reads "Stick · …". |
| `B03-wireframe.png` | The note reads "Wireframe · …". |
| `B04-spacefilling.png` | The note reads "Space-filling · …". |
| `B05-glossy.png` | Ball & stick, Glossy. |
| `B06-hydrogens-polar.png` | Glossy with Polar: **only the carboxylic O–H hydrogen remains**. |

### Run C: dark

| File | What it shows |
|---|---|
| `C01-dark-stick-selected.png`, `C02-dark-stick-deselected.png` | Dark app UI with a white canvas, which is the document's canvas theme. The 3D model section is legible in dark, and the Structure note reads "Stick · …". |
| `C03-dark-canvas-stick-selected.png`, `C04-dark-canvas-stick-deselected.png` | The status-bar sun toggle switched the canvas to dark: "Dark canvas · Copies retain ink colors on a transparent background". The Stick model reads well on black, and the hydrogens are clearly visible. |
| `C05-dark-canvas-ballstick.png` | Ball & stick on the dark canvas. The dark Flat outlines nearly vanish against black, but every atom is distinct. |

## Problems and things worth a look (not fixed)

1. **At Perspective 100% the selection frame disappears** (`A09`).
   - **Measured:** a pixel scan finds no frame-colour pixels in the canvas, against 1,586 in `A01` and `A09b`.
   - **What it looks like:** the floating bar jumps to the canvas bottom, as if the frame had grown far larger than the viewport.
   - **Recovery:** the model is still selected (Properties shows 14 atoms), and Undo brings the frame back.
   - **Likely cause:** the frame or selection bounds may be computed from a perspective projection that blows up at 100%. This is worth checking against the 1b promise that the frame keeps its size.
2. **A small dark notch replaces the seam on outlined (Flat) half-bonds** (`crops/A16-cn-bond-before-after-x3.png`).
   - **Where:** a short stretch of darker, thicker outline where the two half-bonds now overlap. It is about 15 device px at 400%, on the grey half's edge next to the colour change.
   - **Likely cause:** the two outlines stack in the overlap.
   - **Severity:** much less visible than the old pale line, and only seen when zoomed in. It is faintly visible in the 1200 dpi export too.
3. **The Structure note mentions the 3D style even when only 2D is shown.** It reads "Ball & stick · Chemistry and atom IDs stay the same". This is visible in the 1b run (`../ev386-1b-final/app/01`, `11`, `14`) and is cosmetic.
4. **Look does nothing in Wireframe**, and **dark Flat outlines vanish on a dark canvas** (`C05`). Both are consistent with the prototype.
5. **Known, not phase-2 issues:**
   - The keyboard hotspot crosshair sits on the model (`A01`–`A06`, `C01`).
   - The status-bar zoom field is not editable, and `+`/`−` step multiplicatively, so 400% needed the Commands palette.

## Not verified

- Depth fade was not dragged in this run; the previous run covered it.
- Reset view after non-default Look, Hydrogens or Background (it was tested only on the rotation).
- Save and reopen.
- A 1× (non-Retina) display.

## Fix check: frame at Perspective 100% (`app-fixes/`, `ce7d8b22`)

- **Fix:** `ce7d8b22` sizes the frame from the sphere's true projected spread, so at 100% the frame should stay under 2× the flat frame. Before, it was about 6× and left the view (problem 1, `app/A09`).
- **Binary:** `feat/3d-models-2` at `ce7d8b22`, clean tree, built with `CARGO_INCREMENTAL=0`. The sha256 is `bec9623e…` (`app-fixes/binary-sha256.txt`), re-checked right before each launch.
- **Bundle:** `scratchpad/apps-appfix-2/ReShiki386Fix2Evidence.app`, bundle id `dev.reshiki.verification.ev386fix2` (`app-fixes/tools/bundle-Info.plist`). `Contents/MacOS/reshiki` is a hard link to the binary.
- **Data dir:** `scratchpad/ev386-2-appfix-data`.
- **Input:** `input/own/caffeine-ballstick-flat.rsk`.
- **Launches:** two, one after another, with `open -n`:
  - Run 1: PID 82652, window 7762.
  - Run 2: PID 90582.
- **Driving:** Codex computer use with `codex exec -m gpt-6-astra -s danger-full-access … < /dev/null`.
  - Prompts: `app-fixes/prompt.md` and `prompt-zoomed-out.md`. Both runs are in `app-fixes/report.md`.
  - All shots are window-only.
- **Cleanup:** each run killed only its own PID. 51217 was untouched (`app-fixes/process-cleanup.txt`).
- **Measurements:** I re-measured the frames independently from exact frame-colour runs.

**Opening zoom:** the file now opens at **175%**, not 158%. The fix also tightens the frame at 20%, so the fit zoom is higher.

### Run 1: opening zoom, 175%

| File | What it shows |
|---|---|
| `P01-perspective-20.png` | 20%: a full frame of 936×936 px, with the bar just below it. |
| `P02-perspective-50.png` | 50%: a full frame of 943×944 px (1.01×), with the bar just below it. |
| `P03-perspective-100.png` | 100%: the model is fine, but **only the frame's right edge is in view** (x=1816 px), and the bar sits at the canvas bottom. From the 50% centre, the half-width is about 820 px, so the frame is about **1.75×** the 20% frame. That is within the fix's under-2× bound, but at 175% a frame about 1640 px tall cannot fit the 1256 px canvas. Old `app/A09` had **no** frame pixels at all at 158%. |
| `P05a-…`, `P05-perspective-100-rotated.png` | 100%, before and after a rotation: the same clipped frame. |
| `P04-back-to-20.png`, `P06-after-two-undos.png` | Back at 20%, by drag and by Undo: the frame is pixel-identical to `P01`. `P04a`–`c` are intermediate slider values (22% and 21%). |

### Run 2: zoomed out to 112%, two status-bar minus clicks (`Q00` is the 140% step)

| File | What it shows |
|---|---|
| `Q01-perspective-20-zoomed-out.png` | 20%: frame (720,684)–(1335,1299), 615 px. The bar is above the frame. |
| `Q02-perspective-50-zoomed-out.png` | 50%: 620 px, with the bar above. |
| `Q03-perspective-100-zoomed-out.png` | **100%: all four frame edges are in the canvas around the whole model**, (494,458)–(1561,1525) and 1067 px, which is **1.73×** the 20% frame. The bar sits just below the frame (15 px gap, 10 px above the canvas bottom), because there is no room above. |
| `Q04-perspective-100-rotated-zoomed-out.png` | Rotated at 100%: the model is near edge-on, and the frame is unchanged and still around the model. |
| `Q04a-after-one-undo-zoomed-out.png`, `Q05-back-to-20-zoomed-out.png` | Undo of the rotation, then dragging back to 20%: the frame is identical to `Q01`. |

**Verdict:** the fix works. At 100%, the frame stays around the model at about 1.73× the flat size, instead of about 6×.

**Things worth a look:**

- At the opening zoom of 175%, the 100% frame is still taller than the canvas. The user sees one edge and a bar clamped to the bottom (`P03`), which looks like the old bug until they zoom out.
- At 100%, the frame is loose: a wide empty margin around the model in `Q03`. That follows from the bound being for any rotation.
