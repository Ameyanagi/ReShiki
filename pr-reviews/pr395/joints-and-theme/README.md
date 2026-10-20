# PR #395 evidence: mitered Stick joints, theme colours, canvas outlines

- after: `feat/3d-models-2` at b4e35b9e (`after-binary.txt` has the sha256 of target/debug/reshiki).
- before: ce7d8b22, built in a temporary detached worktree; that worktree and its target were deleted (`before-binary.txt`).
- inputs (`input/`, `tools/make_inputs.py`): the ev386-2 own-conformer caffeine and ibuprofen files, Flat look,
  Stick and Ball & stick x light/dark canvas x Publication/Jmol theme (16 files).
- per input, in `before/` and `after/` (`tools/render.sh`):
  - `<name>.svg`, `<name>-export-1200dpi.png`: the repo CLI's `convert` (export paints the canvas background).
  - `<name>-100pct.png`: the 1200-dpi export resampled (Lanczos) to 1 px per drawing unit (canvas 100% on a 1x screen).
    `render` was not used: it always paints white, even for a dark canvas.
  - `<name>-400pct.png` and `<name>-400pct-joint.png`: 4 px per unit, and a 160 px crop centred on a ring atom with three
    heavy neighbours, located with `tools/joint.py` (a port of View::project, checked against the SVG ball centres to 1e-5).
- contact sheets: `sheet-caffeine.png`, `sheet-ibuprofen.png` (before/after at 100%, before/after joint crops shown 2x).
