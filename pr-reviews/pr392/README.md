# Evidence for ReShiki #388: smaller circled charges

- **Before:** origin/main `036f0c82`. Built with `cargo build --locked --bin reshiki` from a temporary detached worktree. The target dir was `../ev388-main-target`, outside this folder. The worktree has since been removed.
- **After:** branch `fix/smaller-circled-charge` at `8b2f8521`. Built with `cargo build --locked --bin reshiki` in `/Users/shiranui/dev/ReShiki/.claude/worktrees/wf_d5366978-b17-2`, freshly rebuilt before anything here was made.

## 1. Renders through the CLI export path

### Inputs (`inputs/`)
Each input is a CDXML file with a ChemDraw `GraphicType="Symbol"` circled charge, using `represent attribute="Charge"`. The stored size is 7.5 pt, written as the BoundingBox centre-to-edge distance.
- `methoxide.cdxml`: generated with `reshiki --cli convert --smiles 'C[O-]' --to cdxml`. The plain charge text was removed and a `CircleMinus` graphic was added on O.
- `methylpyridinium.cdxml`: built the same way from `C[n+]1ccccc1`, with `CirclePlus` on N.
- `ammonium-chemdraw.cdxml`: a copy of the repo fixture `tests/fixtures/exported-charge-chemdraw.cdxml`. This is a real ChemDraw 26 file containing NH4 with `CirclePlus`.

### Commands
`render.sh <binary> <outdir>` runs the following for each input:
- `reshiki --cli convert` to png. This is the app's Export path and writes 1200 dpi: `*-1200dpi.png`.
- `reshiki --cli convert` to svg and to cdxml.
- `reshiki --cli render` to png. This is the assistant preview at its natural size, which the command does not upscale: `*-preview.png`.

`compare.py`, run with `uv run --with pillow python -I compare.py <this dir>`, does three things:
- **Resized PNGs.** `*-100pct.png` is the 1200 dpi export resampled (Lanczos) to 72 dpi, so 1 pt = 1 px. The `400pct` panels use 288 dpi.
- **Side-by-side images.** `side-by-side/<name>-100pct.png` shows the 100% pair, `-100pct-x4.png` shows those same 100% pixels with nearest-neighbour ×4, and `-400pct.png` shows the enlarged pair. `all-400pct.png` stacks all three enlarged pairs.
- **Measurements.** `measurements.txt` holds the circle diameter and sign bar length, read from the exported SVG paths.

### Results
`measurements.txt`, with a stored size of 7.5 pt for all three drawings:

| | before | after | formula |
|---|---|---|---|
| circle diameter | 7.8 pt | 6.0 pt | 0.52→0.40 × 2 × 7.5 |
| sign bar | 4.5 pt | 3.6 pt | 0.3→0.24 × 2 × 7.5 |

The circle and sign centres are unchanged, so each mark shrinks around its stored position.

`cdxml-charge-diff.txt` shows the input `<graphic>` for each drawing, the before and after exported `<graphic>`, and a full-file `diff` of the before and after CDXML exports. All three exports are **byte-identical**. `SymbolType` and `BoundingBox` match the input, for example `51.499998 23.500000 43.999998 23.500000`, which is 7.5 pt. The CDXML charge size is therefore unchanged. Only the SVG and PNG outputs differ.

## 2. Real app (`app/`)
Codex CLI (`-m gpt-6-astra -s danger-full-access`, computer use) ran in the background with the prompt in `app/prompt.md`. It launched the after binary directly (PID 51019), drew C–O with a circled minus and C–N with a circled plus, zoomed to 400% and took screenshots. It then killed only that PID and saved nothing.
- `report.md` is Codex's own report and `measurements.json` holds its pixel measurements. `raw-captures/` keeps the original JPEG captures, and the PNGs are lossless conversions of them.
- `02-both-100pct.png`: both marks at 100%. The app's canonical SMILES is `C[NH3+].C[O-]`.
- `05-both-400pct.png`: both marks at 400%. The readout shows 400%.
- `03-…`: O alone at 400%. Codex moved the whole drawing rather than panning, because scroll-panning was not controllable.
- `04-…`: N alone at 400%, moved the same way.
- `06-inspector.png`: the O atom selected, with the Inspector showing "Circled charge".
- `01`, `02a` and `02b` show the launch and the palette with "Attach to atoms" turned on.

## Observations
- Marks drawn in the app have no stored `size_pt`. Their size falls back to the label font size (10 pt in JACS/ACS), so the circle is 0.8 × 10 = **8.0 pt**; before the fix it was 10.4 pt. That makes it still slightly taller than the O or N capital: about 1.14–1.17× by Codex's pixel count, and about 1.1× by calculation (8.6 pt including the stroke against a cap height of about 7.2–7.5 pt). ChemDraw-imported marks (7.5 pt stored) now draw at 6.0 pt, smaller than the capital. So a charge drawn in ReShiki is still about 33% larger than the same charge imported from ChemDraw.
- No ChemDraw rendering is available to calibrate 0.40/0.24 against, so the issue's measurement step is still open.
- Not caused by this change, but seen: with only the O⁻ atom selected, the Inspector's "Molecular properties" shows `Selection · HO-`, while the canvas label is O and the whole-drawing SMILES is `C[O-]`.
- The app showed a "Recovery draft – Restore / Dismiss" banner at launch, left over from earlier sessions. Codex did not touch it. Ending the test process with SIGTERM may leave a recovery draft for this unsaved test drawing.

## Refresh (branch at `a723b234`)

**Correction to the first observation above:** a charge drawn in ReShiki does *not* fall back to the 10 pt label font. Every reader of an unsized mark already uses ChemDraw's nominal size, 0.75 × the label font (7.5 pt in ACS), on origin/main as well as on this branch. That covers drawing, hit-testing, placement and clearance (`styled_mark_parts`), arrow anchors, the lone-pair hover ring and CDXML/CDX export. So no code change was needed; the branch adds a regression test that pins this behaviour.

The screenshots in `app/` do not show this branch's geometry. Measured from `05-both-400pct.png`, the circle centreline is 7.8 pt across and the sign 4.5 pt, which is the *old* 0.52/0.3 geometry at 7.5 pt. A Nix-installed ReShiki 0.12.0 (`/nix/store/…-reshiki-0.12.0/…/reshiki`, PID 51217, started 14:02) was running during the Codex session, so the captures most likely come from that window. Codex's 1.14× circle/O ratio matches the **before** export exactly (1.14 below). The app screenshots need to be redone, making sure the window belongs to the launched PID.

- **New input `inputs/app-drawn.rsk`:** C–O with ⊖ and C–N with ⊕, built by `scientific::Drawing::place`, the code the app's `place_graphic` calls with "Attach to atoms" on. Its marks store `size_pt = None`. The temporary harness that built it is kept in `harness/zz_app_drawn_388.rs` and was removed from the worktree afterwards.
- `render.sh` now also renders `inputs/*.rsk`. `before/` (origin/main binary in `../ev388-main-target`) and `after/` (branch binary, rebuilt at `a723b234`) were both re-rendered for all four inputs. `compare.py` rebuilt `side-by-side/` (new: `app-drawn-*.png`; `all-400pct.png` now has four rows).
- **Results (`measurements.txt`):** the app-drawn circle is 7.8 → 6.0 pt and its sign 4.5 → 3.6 pt, the same as the three ChemDraw inputs. CDXML export writes 7.5 pt for the unsized ⊖ and ⊕ both before and after, the same size ChemDraw stores. At 1200 dpi the circle's ink, including the stroke, is 1.14× the O's height before and 0.89× after.
- **Regression test:** `app_drawn_circled_charge_matches_chemdraw_nominal_size` in `tests/scientific.rs` places ⊖ on O via `Drawing::place` in the ACS style. It checks that the circle matches the one drawn for `tests/fixtures/exported-charge-chemdraw.cdxml` (7.5 pt, 6.0 pt circle), and that the CDXML export writes a 7.5 pt `BoundingBox`.
