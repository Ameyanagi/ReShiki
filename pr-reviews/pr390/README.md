# ReShiki #386 phase 1a: before/after evidence

Branch `feat/3d-models-1a` at `4f71c836`, in the worktree
`/Users/shiranui/dev/ReShiki/.claude/worktrees/wf_9a402735-96b-2`.
"Before" is `origin/main` at `036f0c82`, exported with `git archive` to
`scratchpad/ev386-main-src` and built into `scratchpad/main-target`. Neither
build directory is inside this folder.

## 1. Renders (`renders/`)

### How the drawings were made (`harness/ev386_harness.rs`)

The harness was copied into the worktree's `examples/` for one run
(`cargo run --example ev386_harness -- <worktree>/target/debug/reshiki renders/`)
and removed again, so the worktree stays clean. Its output is in `harness/run.log`.

- Caffeine `CN1C=NC2=C1C(=O)N(C(=O)N2C)C` goes through the app's own Insert
  path, `LocalEngine::execute(Request::import_smiles(..))`, which lays out the
  2D drawing. The app's Insert runs no separate Clean Up step, so neither does
  the harness. The result has 14 atoms and 15 bonds.
- The model is built the way the Structure section builds one:
  `models3d::selected_molecule`, then `Prepared::new(..).without_retained_depth()`,
  then `Client::generate(.., ForceField::Mmff94s)` with the branch binary as the
  geometry worker, then `Prepared::model`, `require_version(MODELS_3D)` and
  `models3d::set_display`. MMFF94s went from -107.9 to -123.5 and converged. The
  model has 14 atoms and 10 hydrogens.
- The harness wrote these files, and each one reloads through
  `Document::from_json`:
  - `caffeine-2d.rsk`: version 28, with the model kept and display 2D.
  - `caffeine-3d.rsk`: version 28, display 3D.
  - `caffeine-both.rsk`: version 28, display Both.
  - `caffeine-main.rsk`: the same 2D drawing with no model, marked version 27 so
    `origin/main` can open it.

### How they were rendered (the repo CLI)

- `reshiki --cli convert X.rsk -o Y.{svg,png,pdf}` makes the export figures.
  `reshiki --cli render X.rsk -o Y-preview.png` makes the previews.
- Branch binary: `after-2d.*`, `after-3d.*`, `after-both.*` and
  `after-*-preview.png`. The log is `cli-convert-branch.log`.
- `origin/main` binary: `before-main.*` and `before-main-preview.png`. The log
  is `cli-convert-main.log`.
- `after-nomodel.svg` is the branch rendering `caffeine-main.rsk`. It is
  byte-identical to `before-main.svg`.
- `after-2d.svg` (which has a model, shown as 2D) is also identical to
  `before-main.svg`.

### Results

- `before-main.png`: main's 2D caffeine.
- `after-2d.png`: the same picture. With 2D display the export does not change.
- `after-3d.png`: ball & stick only. It has CPK colours (C grey, N blue, O red,
  H white), shaded spheres, two-colour sticks, dark outlines and 10 explicit H.
- `after-both.png`: the 2D drawing on the left and the model on the right.
- On main, opening `caffeine-3d.rsk` is refused: "This drawing was made with a
  newer version of ReShiki (document version 28)". This is the expected
  forward-compatibility refusal, logged in `cli-convert-main.log`.

### Problem found while rendering (see `cli-molecular-export-branch.log`)

Every branch export of a drawing that has a model, even one shown as 2D, prints
`reshiki: warning: Drawing preserved; chemistry needs review: Invalid drawing:
3D models need document version 28 or later`.

Molecular exports of the same file fail outright, with
`reshiki: error: Invalid drawing: 3D models need document version 28 or later`.
These are `--to smiles`, `mol`, `inchi` and `cdxml`, and
`reshiki --cli analyze caffeine-2d.rsk` also fails. The same drawing without a
model (`caffeine-main.rsk`) exports fine on the branch.

Likely cause: `crates/model/src/chemistry/document/output.rs` around line 501
rebuilds `Document { version: EDITOR_BASELINE, atoms, bonds, ..base.cloned() }`.
That copies `models` into a document below version 28, so `validate()` rejects
it.

`crates/model/src/reactions.rs:336` (`candidate.version = REACTIONS`) is a
similar version downgrade and may hit the same error on a drawing with a model.
I did not test it.

## 2. Real app (`app/`)

Codex computer use (`codex exec -m gpt-6-astra -s danger-full-access`) drove the
branch binary `target/debug/reshiki`. The prompt is `app/prompt.md`. Codex's own
report is `app/report.md`, its final message is `app/codex-final.md`, and the
raw log is `app/codex.log`. The verdicts below are mine, judged from the
screenshots.

How it was launched: Codex launched the branch binary directly (PID 72808), but
the computer-use tool could not attach to a binary outside an app bundle. When
Codex looked the app up by name instead, macOS opened the *installed* ReShiki
(PID 73776). Codex had started that instance itself and quit it afterwards. The
seven `wrong-instance-*.png` files are from it: no Structure section, as on
main. Ignore them as evidence for the branch. Codex then made
`ReShiki Verification.app`, whose `Contents/MacOS/reshiki` is a symlink to the
branch binary (`binary-verification.json` checks it is the same file and gives
its sha256), and drove that. No other app was closed, and the worktree stayed
clean (`cleanup-verification.json`).

| Screenshot | What it shows (my reading) |
|---|---|
| `01-launched.png` | Empty branch window. |
| `02-caffeine-inserted.png` | Caffeine inserted through the Import panel (SMILES + Cmd+Enter) and selected. |
| `03-structure-section-2d.png` | Whole molecule selected: "14 atoms · 15 bonds", a **Structure** section with 2D / 3D / Both, and 2D active. Molecular properties shows "Selection · C8H10N4O2". |
| `04a-immediate-3d.png` | First screenshot after clicking 3D. Generation had already finished, so the "Generating…" state was not captured. |
| `04-3d.png` | 3D active. The ball & stick model is in a square frame with no resize handles, and the 2D drawing and its labels are gone. Status: "Shown as 3D model · Undo restores the 2D drawing". |
| `04b-3d-closeup.png` | The same at 247%. |
| `05-both.png` | Both active. The 2D drawing is on the left and the model on the right, inside one selection box. Status: "Shown as 2D and 3D · Undo restores the previous view". |
| `06-2d.png` | 2D active. Only the 2D drawing, in place. Status: "Shown as 2D · Undo restores the 3D model". |
| `07-undo.png` | After one Cmd+Z: Both again, with the same layout as `05`, Redo enabled, and status "History restored". `07-undo-accessibility.txt` confirms Both is on. |

Things that look wrong or worth a look:
1. **Molecular properties loses the formula once a model exists.** It reads
   "Selection · C8H10N4O2" in `03` but only "Selection" in `04`–`07`. This is
   consistent with the analyze failure in section 1, where any chemistry request
   on a drawing with a model is rejected.
2. **Bond sticks are drawn over the front of the atom spheres**, as far as the
   sphere centre. This shows on the carbons in `04b` and in
   `renders/after-3d.png`, for example the methyl carbons and the C=O carbons,
   so it is not canvas-only. It may follow the prototype's back-to-front sort,
   but it looks like sticks piercing the balls.
3. **In Both mode the selection box has 8 resize handles** (`05` and `07`); in
   3D mode it has none. `models3d::shows_3d` counts only `Display::ThreeD`. This
   may be intended, but the summary said "Selections that include a molecule
   shown in 3D have no resize handles". I did not test dragging them.
4. **The 3D frame is much larger than the model** (`04`), leaving wide empty
   margins. This is cosmetic.
5. **The status text after Both → 2D says "Undo restores the 3D model"**, though
   Undo restores Both. The wording is imprecise.
6. The keyboard-drawing hotspot (orange crosshair) sits in empty canvas, and in
   `04` it is clipped at the top edge. This was already listed as a known 1a
   limitation.


## 3. Refresh after the fixes (branch at `b016749f`)

The `after-*` renders, `cli-convert-branch.log` and
`cli-molecular-export-branch.log` were regenerated with the fixed binary.
`before-*` files are unchanged.

- No export warns "chemistry needs review" any more. SMILES, MOL, InChI and
  CDXML exports and `analyze` succeed for `caffeine-{2d,3d,both}.rsk`, giving
  formula C8H10N4O2.
- `after-3d.png`: each ball now covers the sticks that point toward the viewer.
  Sticks no longer run over a ball to its centre.
- `after-2d.svg` and `after-nomodel.svg` are still byte-identical to
  `before-main.svg`.
- `caffeine-both.rsk` keeps the Both offset stored by the old build, so
  `after-both.*` still shows the old wider gap. `caffeine-both-refit.rsk` is
  `caffeine-2d.rsk` switched to Both by the fixed `models3d::set_display`. Its
  renders, `after-both-refit.*`, show the fitted frame placed one bond length
  right of the drawing.
