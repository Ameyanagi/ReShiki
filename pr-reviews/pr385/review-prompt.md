Review the uncommitted changes in this repository (run `git diff` and look at the untracked files under tests/fixtures/chemdraw-attachments/label-justification-*.cdxml). Read-only: do not edit files.

Context: GitHub issue #378. ReShiki (Rust) copies drawings to ChemDraw as CDX/CDXML. Free-text labels (e.g. MeO) have a "Bond attachment" setting Auto/Left/Center/Right/Character N; contracted groups (Boc) have an alignment Auto/Left/Center/Right/Above. Previously ReShiki wrote only the <t>'s LabelJustification (0x0823). Ground truth from ChemDraw 26.1:
- ChemDraw's own saves (the new fixtures) store the Text menu choice as the node's LabelDisplay (0x0401) = Left/Center/Right plus the text's LabelJustification; LabelAlignment (0x0705) is Right for all choices.
- Pasting ReShiki-like clipboard data into ChemDraw and copying back: with only LabelJustification (or + Justification, or + LabelAlignment) every choice came back as Automatic. With the node LabelDisplay set, Left/Center/Right (MeO) and Left/Center/Right/Above (Boc) came back unchanged.
- ChemDraw interprets a flush-right MeO as a methoxy Fragment, which ReShiki imports as a right-aligned contracted group.

The change: write node LabelDisplay for explicit free-text attachments (overriding the hydrogen-position LabelDisplay that labels() writes for every atom), write node LabelDisplay on group Fragment nodes for explicit alignments, accept LabelDisplay="Center" on import (previously an error), tests, docs and changelog.

Look for: correctness bugs, cases where the new LabelDisplay could be wrong (e.g. atoms where the text was not written, generic atoms, variable_labels=false, hidden labels, file export vs clipboard export), regressions in hydrogen-position export/import, whether "Center" mapping to HydrogenPosition::Auto can hide a real problem, test gaps, and inaccurate docs claims. Report concrete findings with file:line and severity; say plainly if there are none.
