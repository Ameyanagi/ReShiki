# Atom labels and owned indicators — 2026-09-20

Properties → Labels & chemistry → **Atom labels & numbering…** opens the
display controls. Choose Whole drawing or Selected atoms, then choose skeletal,
terminal, internal or all carbon labels. Implied hydrogens can be shown or hidden,
with automatic, left, right, above or below placement on the affected atoms.
These are display settings: changing them does not change the molecular graph
or its formula.
Drawing carbon/H/stereo defaults apply to newly added atoms. Hydrogen placement
is a per-atom choice applied to the current scope.

To switch a hydroxyl label between **OH** and **HO**, select its oxygen and open
**Properties → Labels & chemistry → Atom labels & numbering…**. With
**Selected atoms** as the scope, choose **Right** for OH or **Left** for HO under
**Hydrogen position**. Restore **Automatic** to follow the surrounding bonds.
See the [illustrated OH/HO walkthrough](https://reshiki.com/guide/labels/#change-oh-to-ho)
for the controls and an editable example.

Automatic placement uses the internal bond angles to choose an inline or stacked
label. With two bonds it uses their angle bisector, including a stable choice
for straight bonds. Terminal labels keep the usual inline
form. It updates when a structure moves or rotates; explicit placement overrides
it. Condensed display labels such as CCl₂ and NMe use the same placement logic,
keeping the attachment element at the vertex and subscripts with their label.
Unknown names remain intact. See the [internal-label examples and scope](changes/automatic-hydrogen.md).

Bonds meet the attachment atom inside a multi-atom text label (MeO, AcS,
SPh), not the label's center. Automatic attachment skips substituent
nicknames (Me, Ac, Bn, Ph, TBS…), hydrogens and counts, and uses the atom
nearest the bonds: the first atom when they arrive from the left, the last
when they arrive from the right (the first atom for vertical bonds). Text
naming no atom (Boc, M) stays centered, unless a long automatic terminal
label would hide more than half of its single near-horizontal bond; that label
meets the bond with its near edge. Text whose other letters would lie between
the atom and a horizontal bond (MeO bonded from the left) also stays centered,
so those letters do not hide the bond. Automatic labels with two or more bonds
stay centered too. Contracted groups use the same rule on their displayed spelling, so
a group without a reverse spelling still attaches at its anchor (OAc read
from the right attaches at O, not at Ac). **Bond attachment (text labels)**
overrides it per atom: **Centered** centers the label, **Flush left**/**Flush right** attach at the first or
last atom, and **Character N** at that character. Native files store the
choice as `label_attachment`. Editable CDXML/CDX clipboard data writes flush
left/center/right as the text's `LabelJustification` and the atom's
`LabelDisplay`, as ChemDraw saves its **Text → Flush Left / Centered / Flush
Right**; ChemDraw lays a label without `LabelDisplay` out again as Automatic.
Import reads `LabelJustification` (CDXML/CDX file export still refuses free
text labels). Copying to ChemDraw 26.1 and back was checked on macOS: Flush
left and Centered return unchanged. Flush right MeO returns as a contracted
methoxy group aligned flush right, because ChemDraw reads a flush-right MeO
as a methoxy fragment (it does the same for labels drawn in ChemDraw). A
chosen character has no ChemDraw equivalent and is native only; changing the
text returns it to Automatic.

Alternative lists such as `[Cl,Br,I]` with a single horizontal bond meet it
with their near edge, so the text extends away from the molecule and the bond
is trimmed at the label instead of running through it. Vertical and internal
generic labels, including link nodes, stay centered.

Number assigns one sequence in creation/import order. Starts include `1`,
`atom1`, `a`, `A`, `α` and `Α`; letter sequences continue beyond the alphabet.
A single selected atom also accepts a custom number such as `Cα` or `12a`.
Numbers are owned by atoms, independent of reaction atom-map numbers.

**Show stereodescriptors (R/S, E/Z…)** displays computed CIP assignments. Chemical edits
invalidate cached assignments and trigger a quiet background refresh. Stale
responses are ignored and rescheduled; refresh does not change selection,
document dirtiness, drawn bond orders or Undo/Redo. Invalid chemistry produces
an inspector error. Unassigned centers have no label.

Enantiomorphic double bonds display _seqCis_/_seqTrans_; native files and
machine-facing exports retain the internal `z`/`e` codes. **Axial descriptors**
chooses **M/P (IUPAC preferred)** or **Ra/Sa** for the whole drawing, independently
of the selected scope. M maps to Ra and P to Sa; lowercase m/p map to ra/sa.
The choice changes display only and can be undone. Every stereodescriptor uses
the same italic indicator style. This option does not add axial stereo perception.
Search **CIP** or **pseudoasymmetric** in the command palette (`:`) to show or hide
stereodescriptors throughout the drawing.

**Show D/L for amino acids and sugars** is off by default and applies to the
whole drawing. When stereodescriptors are visible, recognized structures show
an upright D or L before their CIP assignment, for example L (_S_). The CIP
code keeps its italic style; **Upright stereo parentheses** also keeps the
parentheses upright. Search **D/L** in the command palette to toggle this display.
The choice is saved in native documents and does not alter stereochemistry.

For an alpha amino acid with N, C(=O)X (X = O, N, S or H, not a ketone) and
carbon side-chain neighbors plus one hydrogen, D/L follows the assigned R/S code
and the carbonyl versus side-chain CIP priority. This includes peptide residues
and handles L-cysteine as L (_R_). The comparison uses atomic numbers through
the second sphere, including multiple-bond duplicates and hydrogens. A tie
omits D/L rather than guessing from incomplete priorities. Glycine and
unrelated stereocentres keep their usual indicators.

Recognized Haworth monosaccharides show D/L only at the last configurational
carbon. If ring closure involves a terminal CH2, the preceding stereocentre
supplies the name. Open-chain aldoses with stored configurations, such as the
Fischer templates, show D/L at the stereocentre next to CH₂OH. Both forms read
stored stereochemical parity and connectivity, so turning a Fischer projection
by 90° (which draws the enantiomer) changes D to L rather than hiding the name.

Indicators start at 7.5 pt Arial; stereo labels are italic. Change their size or
apply the current toolbar font/color. **Position numbers & stereo labels**
shows handles: drag a number or stereo indicator, then Escape to finish. Each
drag is one Undo step. Automatic positions consider nearby labels, captions and
bonds. A custom number longer than one character starts beside its atom on the
open side and is left- or right-justified so extra text runs away from the
structure; above or below an atom it stays centred. Restore automatic positions
removes manual offsets; a dragged indicator keeps its exact position. Copying, grouping and
transforming a structure retain indicator ownership.

Canvas, SVG, PDF and PNG share the indicator geometry. Drawing exports refresh
computed labels on a snapshot before rendering. JACS remains the new-document
default: Arial 10 pt atom/caption text, black, 14.4 pt bonds and 0.6 pt lines.
New clears label overrides and restores skeletal carbons, visible implied H,
and no atom numbers or stereo indicators. Native format 9 stores the settings,
styles and offsets; previous supported document versions remain readable.

CDXML imports and exports use node/bond-owned `objecttag` records,
plus label-display attributes. Atom numbers do not become detached captions.
Imported CIP text is never trusted as chemistry; Rust recomputes the assignment.
Desktop exports provide measured positions. Protocol clients that omit the
`atom_indicators` payload receive a simple positional fallback.

CIP assignments use the Rust implementation, independently checked against [RDKit's CIP labeler](https://www.rdkit.org/docs/source/rdkit.Chem.rdCIPLabeler.html). The numbered phenylalanine interchange fixture retains `C9H11NO2`, the same InChIKey, 12 atom-owned numbers and an `(S)` label.

Limits: tetrahedral R/S/r/s, ordinary double-bond E/Z, enantiomorphic
double-bond seqCis/seqTrans, biaryl atropisomer M/P and three-carbon allene
M/P (m/p for pseudoasymmetry) are supported. Biaryl
axes require an authored wedge or hash originating at an axis atom; distant
substituent wedges do not specify the axis. Rotation barriers are not inferred.
Allene stereo requires terminal substituent wedges at one end only; wedges at
both ends receive an ST-1.6 drawing note. Its indicator belongs to the central
carbon. SMILES retains `@AL1`/`@AL2`, and MOL/CDXML retain terminal wedges.
Longer cumulenes are not implemented.
InChI generation uses verified terminal wedges with real 2D coordinates;
coordinate-free or projected allene input cannot use that path. The raw InChI
adapter maps allene parity, but native InChI import still discards it with a
diagnostic because the pinned dependency does not expose its parity records.
Naming still excludes allene stereo. Unknown object tags, unsupported indicator
formatting and ambiguous atom matches are
rejected. Dense diagrams can still need manual indicator positioning. Numbering
is an explicit command; it does not automatically renumber after every edit.
Further features remain listed in the [feature status](feature-status.md).

Original update validation: 125 Rust and 36 Python tests passed, covering identity, sequences,
native/CDXML ownership, transform/copy, exported geometry, stale refreshes and
history. Native computer checks cover the imported interchange fixture, numbering,
manual drag, Undo/Redo, native persistence and external reopening of the export.
Local artifacts and logs: `artifacts/atom-labels-qa-20260920/`.

## Enhanced stereo groups

Enhanced stereo groups record what is known about a sample beyond its drawn
wedges:

| Label  | Group       | Meaning                                                                                             |
| ------ | ----------- | --------------------------------------------------------------------------------------------------- |
| `abs`  | ABS         | The configuration is exactly as drawn, for example (_S_)-alanine.                                   |
| `and1` | AND 1 (rac) | A 1:1 mixture of the drawn form and its mirror image at these centres, for example racemic alanine. |
| `or1`  | OR 1 (rel)  | One of the two forms, not known which; the centres keep their relation to each other.               |

Centres with the same label flip together; different groups vary
independently. A stereocentre without a group is absolute, so existing
drawings and new ones keep their meaning.

Select stereocentres (or whole molecules) and open **Stereo groups** in the
inspector. Choose **abs**, an existing group, **New AND group** or **New OR
group**; only atoms with an assigned R/S descriptor change. Each change is one
Undo step. **Renumber groups** appears when the numbers have gaps and numbers
the AND and the OR groups of the drawing 1, 2, 3… in order.

Numbers belong to the whole drawing: equal labels anywhere are one group.
Pasting or inserting a structure gives its groups numbers the drawing does not
use yet, so a pasted racemate never joins an existing group. Copy and paste,
Undo/Redo, Clean up and flipping keep the groups; flipping a racemate or a
relative structure leaves it racemic or relative.

Group labels are shown by default as plain 7.5 pt text beside the centre:
`and1` and `or1` on grouped centres, and `abs` on the other stereocentres of a
molecule that has a group. **Show stereo groups (abs, and1, or1)** under
Atom labels & numbering hides or shows them for the scope, and the command
palette has **Show/Hide stereo groups**. R/S descriptors stay as computed for
the drawn form. The labels are positioned, styled and exported like the other
indicators.

Formats:

- MOL V3000 and RXN V3000 read and write `MDLV30/STEABS`, `STERACn` and
  `STERELn`. A drawing with groups is written as V3000, because V2000 cannot
  hold them; ungrouped centres stay in `STEABS`.
- SMILES output adds a CXSMILES extension, for example
  `C[C@H](N)C(=O)O |&1:1|`, with `a:` for the ungrouped centres. CXSMILES input
  reads `a`, `&n` and `on`. A relative flag (`|r|`, or `r:` on fragments)
  imports as an AND group of the configured centres, as RDKit reads it.
- CDXML and CDX, including the clipboard, read and write `EnhancedStereoType`
  and `EnhancedStereoGroupNum`.
- InChI and naming do not support the groups yet and say so instead of giving
  an identifier for the drawn enantiomer.
