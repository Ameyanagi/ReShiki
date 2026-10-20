# Evidence for ReShiki #387 (orbiting atom marks)

Branch `fix/atom-mark-orbit-drag` at `cb905a76` (worktree `.claude/worktrees/wf_d5366978-b17-1`)
vs. "before" = `origin/main` at `036f0c82` (the merge base; origin/main had not moved).

## 1. Renders (`before/`, `after/`, `comparison.png`)

How they were made:

- `harness/ev387_harness.rs` is a throwaway integration test. It was copied into `tests/` of each tree, run once,
  then deleted (nothing committed). The `before` run used a temporary detached `origin/main` worktree, removed afterwards.
  - Command: `RESHIKI_INCHI_HELPER=<branch target>/reshiki-inchi-helper EV387_OUT=<dir> cargo test --test ev387_harness`.
- The test imports SMILES `CO` and uses the symbol tool's own placement code, `Drawing::place` with Attach on,
  to drag four marks out of the O. The press is on O and the release is 0.6 bond lengths away (25.2 world units).
  - Order: Circled minus to θ+180°, then Lone pair to θ, θ+120° and θ+240°.
  - θ ∈ {0, 45, 90, 135, 200}. Angles are on screen, counter-clockwise from +x.
  - Main and the branch got exactly the same inputs. On the branch, `place()` routes a drag through the new `orbit_mark`.
    On main it keeps the raw release offset.
- `methoxide_045_rot30.rsk` is the θ=45 drawing rotated 30° with `editing::transform(.., Transform::Rotate(30.))`.
  That is the Rotate command path, which goes through `map_positions`.
- Each `.rsk` is the document's `file_json()`, the same bytes the app saves. It was exported with the repo CLI:
  `reshiki --cli convert X.rsk -o X.png` (the app's Export, 1200 dpi) and `-o X.svg`.
  - Before files were exported with a main build of `reshiki` and after files with the branch build.
    The branch does not touch render or export code anyway.
- `marks.txt` in each folder lists every mark's offset, its distance from O, its direction, and the stored `angle`.
- `comparison.png`: top row is main and bottom row is the branch, with every panel at the same scale.
  It was built with `harness/compose.py` (Pillow via `uv run --with pillow`).

## 2. Real app (`app/`)

The branch binary from `cargo build --bin reshiki` was copied to `appbin/` and driven by Codex computer use
(`gpt-6-astra`, `-s danger-full-access`). The prompt is in `app/prompt.md` and the report in `app/report.md`;
the screenshots use the step-numbered names given in the prompt.

- First run (`app/run1-blocked/`): blocked. The user's installed ReShiki was already running and computer use could
  not target the bare binary. Codex touched nothing.
- Second run: the same branch binary wrapped in `appbin/ReShikiEv387.app` (bundle id `dev.reshiki.ev387branch`,
  ad-hoc signed). Held-button and Option drags were posted as CGEvents to that window only.
  The Codex run wrote `app/report.md`. Every screenshot there is a window-only capture
  (`screencapture -l <ev387 window id>`).
  - `ev387_guard.py` and `ev387_drag.py` are its guarded CGEvent scripts; the `*.json` files log each drag path.
  - `00-*.png` are setup shots.
  - `01`–`08` follow the flow: C–O, then marks added, orbit drag mid-way (315°, 270°), release,
    Option drag (mid-drag and released), charge drag (mid-drag and released), before/after rotate, and Undo.
  - Codex quit only its own instance. The user's installed ReShiki (PID 51217) was not touched.

## What looked wrong (judged from the images, not from Codex's prose)

1. **Charges sit much farther out on diagonals.** A circled charge's orbit is a rectangle (see `app/06-charge-mid-drag.png`),
   so diagonal snaps sit at the box corner.
   - In the renders the charge is 24.2 from O on the axis and 34.3 on a 45°/135° diagonal (`after/marks.txt`).
     That is visibly detached in `after/methoxide_045.png`, `after/methoxide_135.png` and `after/methoxide_045_rot30.png`.
   - This contradicts the docs' "the gap stays even".
2. **The lone-pair orbit guide is jagged.** The dashed line zig-zags around the O, and several 15° snap dots
   sit visibly off it (`app/03-lonepair-mid-drag.png`, `app/03b-lonepair-mid-drag-2.png`).
3. **The orbit ignores bonds.** Snaps toward the C–O bond put the mark on the bond line:
   - lone pairs at 165° and 195° touch the bond end in `after/methoxide_045.png`, `after/methoxide_200.png`
     and `after/methoxide_045_rot30.png`;
   - the charge at 180° sits on the bond in `after/methoxide_000.png`;
   - the charge orbit's 180° snap dot lies on the bond in `app/06-charge-mid-drag.png`.
   Main also lets a drag land on the bond, but it does not pull marks onto it.
4. **The orbit ignores other marks.** A lone pair dragged to 270° (top) overlaps the circled minus already there
   (`app/03b`, `app/04-lonepair-released.png`).
5. Not new: the three click-placed lone pairs put two pairs on the right side at different radii (`app/02-marks-added.png`).
   That is click placement, unchanged by this branch.

Confirmed OK:
- 15° snapping and the "Orbit · N° · dots tangent / upright" and "Free · N°" notices.
- Lone pairs stay tangent to the atom; charges stay upright.
- Straight up gives the clicked position.
- An Option drag keeps the exact offset.
- No jump between the preview and the release.
- Rotate turns lone pairs and keeps charges upright, both in the app and in the CLI renders.
- One Undo restores the rotation.

Not verified: the Shift variant of the free drag, all 24 snap positions, and CDXML export of these drawings.

## Refresh (branch at `5eb91cc2`)

The fixes are in `de91d7cf`, `44be117a`, `fb662040` and `5eb91cc2`. `after/*.rsk`, `after/marks.txt`, `after/*.png|svg` and
`comparison.png` were regenerated the same way: the same harness, a fresh `cargo build --bin reshiki`, and the same `compose.py`.
`before/` was not touched. The `app/` screenshots still show the earlier branch build (`cb905a76`) and have not been retaken.

- **Even gap.** The charge is now 24.6–25.1 from O at every snap; it was 24.2 on the axes and 34.4 on diagonals.
  Lone pairs are 15.1–16.0; they were 15.1–21.3.
  - Each glyph is a rounded rectangle fitted to its outline, so O is nearly round and N/H keep their corners.
    The label's glyphs together are wrapped as one convex outline, so OH reads as one rounded shape.
  - The gap is measured from that outline to the mark's outline.
    The unit test `the_orbit_gap_to_the_label_ink_stays_even_all_the_way_round` measures O, OH and NH2 over all 24 snaps: the spread is at most 6%.
- **Bonds.** Drags toward C no longer land on the C–O bond.
  - θ=0: the charge asked for 180° and went to 225°.
  - θ=45 and θ=200: the lone pairs asked for 165° or 195° and went to 135° or 225°.
  - Snap steps 150–210° are unavailable for both the charge and the pairs, and the guide leaves them out.
- **Not avoided: other marks.** In `after/methoxide_000.png` the lone pair at 240° overlaps the charge, which was pushed from 180° to 225°.
  This is by design: dragging onto another mark is the user's choice.
- The guide fix (smooth line, dots on it) is canvas-only. It is covered by `orbit_guides_are_smooth_and_carry_the_committed_snaps`
  but has not been re-shot in the app.
