# Tunable pen paths (#67)

Review base: `feat/mechanism-curve-controls` at 5e68c9b4 (production 477b97f4).
Pen production reviewed in the desktop app: 2b3fdb40, signed SHA256 b630b1ff119a681d82b35f0a64c20cda92e364d11e70a71f816a8ee9a9e8ed6c.
The later e10c876f parent merge changes only Curves reference-test provenance.
See [the visual review](../../../docs/changes/tunable-pen-lines.md) and
[public artifact hashes](../../../docs/changes/tunable-pen-lines-evidence.json).

`before.rsk`, retained before implementation in f160a13b, contains an open
multi-cubic path with alternating bends and a closed asymmetric outline.
SHA256: 6168c62b8fa319b136fad75071b191108c883b91ca3c95528857621830f7499f.
These Path commands already render/save/reopen at the base. The Pen palette,
node operations and adjacent tangent transport are the new behavior.

At 127% select the upper path and Edit curve points. Drag the interior node
near (95,30) by about (20,-10). The base moves only that point; Pen carries both
adjacent controls, leaving other points and the closed outline fixed. Actual
base/after desktop saves are pen-baseline-node-desktop.rsk and
pen-candidate-node-desktop.rsk, with matching candidate Undo/Redo readbacks.
Those captures/actions belong to historical a7301867, not final 2b3f.

Historical desktop authoring at a7301867 created pen-authored-four-nodes.rsk:
first drag (600,900)→(900,900), click (1200,800), drag (1450,1000)→(1450,1200).
It contains one Path id 1 with move,cubic,line,cubic. The three-node, four-node
Undo/Redo, inserted/deleted, straight/curved and closed/reopened saves retain
actual native actions. No freehand tracing is claimed.

Final 2b3f/b630 checks open the same pen-authored-four-nodes.rsk at 148%, F8 off:

- Edit Points, Node 4 click (1640,1140), same pixel release. The saved
  pen-node-click-final-no-edit.rsk is raw-byte equal to the input, with no history.
  pen-node-click-before-fix.rsk retains the earlier 01b/9e click defect.
- A stationary square-control click saves pen-tangent-click-final-no-edit.rsk
  equal to the input and adds no history. Existing Undo/Redo from earlier actions
  remains; it is not claimed cleared.
- Node/tangent drag screen displacement (+40,-40) saves the respective
  pen-node-offset-drag-final.rsk / pen-tangent-offset-drag-final.rsk. Only the node
  plus incoming tangent, or the one tangent, moves by about (+13.48618,-13.48618).
  Each -undo file equals the original raw input; each -redo equals its edited save.
- Explicitly choose Select, select the path, Continue, save without editing,
  then click (1800,1140). pen-continue-explicit-select-final-{no-edit,appended,undo,redo}.rsk
  retains the original before the append, adds only one line to (236.90263,3.8988686),
  and records one exact Undo/Redo. The old a730 at 148% failed-Continue save is
  pen-continue-select-before-matched-148.rsk, raw equal to the original input.
- Explicitly choose Edit Points, stationary Node 4 selection, Continue, save,
  then click (1780,1220). pen-continue-explicit-edit-points-final-{no-edit,appended,undo,redo}.rsk
  adds only one line to (230.15955,30.871231) with the same raw no-edit/Undo/Redo guarantees.
- Fresh-process reopen of that five-node file, fit 136%, exposes five nodes and
  editable controls with Undo/Redo disabled. pen-final-five-nodes-fresh-reopened.rsk
  is raw-byte equal to the appended file.

All final drawings retain one original graphic id 1 and every other serialized
field. The fixtures contain no atoms/bonds/arrows/annotations/groups. App tests
separately prove preservation of an unrelated chemical graph and projected
path frames. Escape cancels a gesture. Existing arc controls remain available.
Supported native/CDXML/CDX path geometry and PNG/SVG/PDF output were tested;
unsupported editable decorations keep explicit rejection.

Native files and screenshots are original ReShiki desktop outputs. Their bytes
were retained, independently audited and never retouched. Earlier exploratory
candidates are not published as correctly routed final evidence.
