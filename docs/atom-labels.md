# Atom labels and owned indicators — 2026-09-20

Properties → **Atom labels & numbering** opens the display controls. Choose
Whole drawing or Selected atoms, then choose skeletal, terminal, internal or
all carbon labels. Implied hydrogens can be shown or hidden, with automatic,
left, right, above or below placement on the affected atoms. These are display
settings: changing them does not change the molecular graph or its formula.
Drawing carbon/H/stereo defaults apply to newly added atoms. Hydrogen placement
is a per-atom choice applied to the current scope.

Number assigns one sequence in creation/import order. Starts include `1`,
`atom1`, `a`, `A`, `α` and `Α`; letter sequences continue beyond the alphabet.
A single selected atom also accepts a custom number such as `Cα` or `12a`.
Numbers are owned by atoms, independent of reaction atom-map numbers.

**Show R/S and E/Z labels** displays computed CIP assignments. Chemical edits
invalidate cached assignments and trigger a quiet background refresh. Stale
responses are ignored and rescheduled; refresh does not change selection,
document dirtiness, drawn bond orders or Undo/Redo. Invalid chemistry produces
an inspector error. Unassigned centers have no label.

Indicators start at 7.5 pt Arial; stereo labels are italic. Change their size or
apply the current toolbar font/color. **Position numbers & stereo labels**
shows handles: drag a number or stereo indicator, then Escape to finish. Each
drag is one Undo step. Automatic positions consider nearby labels and bonds;
Restore automatic positions removes manual offsets. Copying, grouping and
transforming a structure retain indicator ownership.

Canvas, SVG, PDF and PNG share the indicator geometry. Drawing exports refresh
computed labels on a snapshot before rendering. JACS remains the new-document
default: Arial 10 pt atom/caption text, black, 14.4 pt bonds and 0.6 pt lines.
New clears label overrides and restores skeletal carbons, visible implied H,
and no atom numbers or stereo indicators. Native format 9 stores the settings,
styles and offsets; previous supported document versions remain readable.

CDXML imports and exports use node/bond-owned `objecttag` records,
plus label-display attributes. Atom numbers do not become detached captions.
Imported CIP text is never trusted as chemistry; RDKit recomputes the assignment.
Desktop exports provide measured positions. Protocol clients that omit the
`atom_indicators` payload receive a simple positional fallback.

CIP assignments use [RDKit's CIP labeler](https://www.rdkit.org/docs/source/rdkit.Chem.rdCIPLabeler.html). The numbered phenylalanine interchange fixture retains `C9H11NO2`, the same InChIKey, 12 atom-owned numbers and an `(S)` label.

Limits: tetrahedral R/S/r/s and ordinary double-bond E/Z are supported. Allene,
atropisomer and enhanced-group stereochemistry are not implemented. Unknown
object tags, unsupported indicator formatting and ambiguous atom matches are
rejected. Dense diagrams can still need manual indicator positioning. Numbering
is an explicit command; it does not automatically renumber after every edit.
Further features remain listed in the [feature status](feature-status.md).

Validation: 125 Rust and 36 Python tests passed, covering identity, sequences,
native/CDXML ownership, transform/copy, exported geometry, stale refreshes and
history. Native computer checks cover the imported interchange fixture, numbering,
manual drag, Undo/Redo, native persistence and external reopening of the export.
Local artifacts and logs: `artifacts/atom-labels-qa-20260920/`.
