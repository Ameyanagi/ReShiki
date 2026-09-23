# ChemDraw attachment comparison

Checked ChemDraw 26.0.0.6599 on macOS on 2026-09-23. Computer Use successfully read the window and operated the Structure menu after its initial authentication error. Native macOS events were used for the floating bond palette and dragging, which Computer Use did not deliver reliably to this app. Only isolated test documents were edited.

| Operation                                                          | Observed result                                                                           | Meaning                                                                     |
| ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| Select six arene atoms → Structure → Add Multi-Center Attachment   | A central asterisk; CDXML `NodeType="MultiAttachment"`, `Attachments="10 11 12 13 14 15"` | One attachment representing the selected atoms together                     |
| Select three consecutive allyl atoms → Add Multi-Center Attachment | A point at their mean position; `MultiAttachment` references precisely the three atom IDs | The operation also works for an open chain, not only a ring                 |
| Select arene atoms → Add Variable Attachment                       | Central asterisk; CDXML `NodeType="VariableAttachment"`, with the candidate atom IDs      | Alternative attachment positions, not simultaneous multicentre coordination |

A bond can start at the multi-centre point and terminate in a metal label. The point is a dedicated node referencing existing atoms, not an extra carbon. Perspective is a separate operation: no tilt was needed to create either attachment type.

The vendor's [ChemDraw user guide, pages 171–173](https://kudos.kindai.ac.jp/cms/pdf/chemdraw_userguide.pdf#page=180) describes polyhapto drawings and positional-isomer notation separately. Arene/allyl haptic coordination needs multi-centre attachment; variable attachment represents the alternative-position case.

ReShiki now has distinct **multi-center** and **variable attachment** nodes, in addition to its older nonchemical drawing centroids and wildcard labels. Select the target atoms, then use Properties → Arrange & transform → Add multi-center attachment / Add variable attachment. Draw a bond from the new target marker to the metal or substituent. Anonymous dummy/attachment markers remain visible in the editor, including when bonded, but are excluded from figure and Office clipboard output. Explicit labels such as M/L/E and chemical abbreviations remain visible. Attachment positions remain editable; only legacy drawing centroids snap to their members' mean position.

The native format retains the target IDs in the existing `centroid` list, plus an explicit `attachment` kind. Old centroids are never silently promoted. CDXML/CDX write `NodeType` and `Attachments`; V3000 writes `ENDPTS` with `ATTACH=ALL` or `ANY`. ALL uses the RDKit dummy-to-metal dative convention. Copying and fragment placement remap target IDs; complete molecular selections include attachment targets. Deleting a target removes the dependent attachment, rather than silently changing its meaning.

Local inspection files are under `artifacts/chemdraw-reference/`; they are scratch documents, not chemical reference structures.

## Chemical meaning versus appearance

The published [CDX/CDXML Node specification](https://chemapps.stolaf.edu/iupac/cdx/sdk/Node.htm) stores label text separately from atomic number, hydrogen count, formal charge and node type. A text label by itself is not proof that the underlying chemistry is correct.

- `Nickname`/`Fragment` nodes carry a nested atom/bond fragment: Boc can be displayed compactly while retaining its structure. Uninterpretable labels can use `Unspecified`; generic/query labels are another category. ReShiki's new free labels deliberately mean named wildcard atoms, not a metal/halogen query inferred from M or X.
- `MultiAttachment` carries all participating atom IDs. `VariableAttachment` carries alternative attachment positions. These are distinct from both a plain wildcard and an abbreviation's external connection point. The [NodeType specification](https://chemapps.stolaf.edu/iupac/cdx/sdk/properties/Node_Type.htm) also permits distributed charges on multicentre nodes.
- Atomic formal charge is a separate [Charge property](https://chemapps.stolaf.edu/iupac/cdx/sdk/properties/Atom_Charge.htm). A captioned overall charge must not silently invent a charge assignment on one particular nitrogen.
- ChemDraw retains drawings with chemistry warnings and exposes Check Structure. Warning suppression is a presentation setting, not validation. Its user guide also distinguishes symbol-tool charges from ordinary text; floating delocalized charges can be lost in formats requiring atom-local charge assignments.
- Curves/circles are drawing objects or depictions. Their presence alone is not a complete aromaticity/valence model. Faithful figure interchange needs to preserve these graphics independently of molecular conversion.

The source-image regression remains a visual test, with an unresolved nitrogen valence assignment and overall charges drawn as captions. ChemDraw's ability to display such a figure does not resolve that chemical ambiguity automatically.

## RDKit 2026.03.6 measurements

Run `uv run --locked python scripts/check_attachment_interop.py --output artifacts/chemdraw-reference/rdkit-attachments.json`.

The script uses synthetic CDXML authored from the public format and independent small molecules. It neither extracts ChemDraw implementation code nor depends on private documents. Static inspection of the installed ChemDraw bundle confirmed a universal executable, native nickname-handling symbols and bundled RDKit libraries; no proprietary implementation was copied or linked into ReShiki.

| Path                                                   | Observed behavior on the locked build                                                                                                                                                                                                                                   |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MolsFromCDXML` and `rdChemDraw.MolsFromChemDrawBlock` | Both lost the MultiAttachment/VariableAttachment member lists in the test inputs. A missing Element attribute defaulted the special node to carbon, yielding `[CH3][Fe].c1ccccc1`. This is not a chemically faithful import.                                            |
| `DativeBondsToHaptic`                                  | Converts contiguous donor atoms into a centre dummy connected to the metal by a dative bond, with `_MolFileBondEndPts` and `_MolFileBondAttach="ALL"`.                                                                                                                  |
| `HapticBondsToDative`                                  | Expands those endpoints back into donor-to-metal dative bonds. Round trips passed for η³-allyl, η⁶-arene and ferrocene.                                                                                                                                                 |
| V3000 MOL                                              | Retains the dative bond with `ENDPTS=(...) ATTACH=ALL`; reloading with sanitization and expanding reproduced the original graphs in all three examples. Compare normalized graphs: unsanitized reloads have Kekulé bonds and are not text-identical to aromatic SMILES. |
| Variable attachment from CXSMILES `m:`                 | Reads endpoint lists with `ATTACH=ANY`; V3000 save/reload retained them. Default `MolToCXSmiles` in this build omitted the `m:` information. Do not use that output as a lossless round trip.                                                                           |

See the [RDKit source and release tag](https://github.com/rdkit/rdkit/tree/Release_2026_03_6) and [RDKit Book's CXSMILES section](https://www.rdkit.org/docs/RDKit_Book.html#cxsmiles-cxsmarts-extensions). These observations describe the tested versions and inputs, not every external application or every organometallic structure.

## Required ReShiki representation

Keep a typed attachment node (`MultiCenter` or `Variable`) with stable member atom IDs and its drawing position. Keep ordinary free labels, query labels and chemical abbreviations distinct. Map multicentre nodes explicitly to RDKit's dummy/dative/ALL representation, and positional alternatives to ANY. Map the same types directly to the CDXML NodeType and Attachments properties.

The centroid's position can serve as a drawing aid, but must not determine chemical meaning. Atom-local versus distributed charge also needs explicit ownership. Preserve partial ring curves as drawing data while retaining separate chemical bond orders.

The app's CDXML preparation now overrides the low-level reader's inert-node behavior before chemical reconstruction, and restores typed targets by source IDs. V3000 import retains ENDPTS/ATTACH. The ordinary molecular graph used temporarily for drawing reconstruction is not a full chemical interpretation: attachment imports and exports deliberately omit molecular analysis. SMILES/InChI and molecular analysis remain unavailable for semantic attachments. RXN attachments are rejected. V3000 requires one bond per point and cannot carry distributed charges/radicals; use CDXML/CDX/native for those. Partial ring curves and free wildcard text still require native/figure output because their CDXML interchange is separate work.

## Executed compatibility checks

`cargo run --example attachment_qa -- artifacts/attachment-qa/final` produces isolated files for GUI checks. The committed `tests/fixtures/chemdraw-attachments/` files were subsequently edited and saved by ChemDraw 26.0.0.6599 using Computer Use, not merely generated by ReShiki.

| Case                                                 | Computer Use / ChemDraw result                                                                                   | Automated regression                                                                      |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| η³ allyl                                             | Opened, edited and resaved                                                                                       | Three targets retained; no extra carbon                                                   |
| η⁶ arene                                             | CDXML and binary CDX opened/resaved                                                                              | Six targets retained, including counted CDX ID arrays                                     |
| Ferrocene                                            | Opened, edited and resaved                                                                                       | Two multi-center points, five targets each                                                |
| Variable arene                                       | Opened, edited and resaved; generic structure command available                                                  | ANY remains distinct from ALL                                                             |
| Boc, Cbz, Fmoc, Ac, Ts, Ms, Me, Et, tBu, Ph, Bn, OMe | Collapsed labels opened; Check Structure reported **No errors found**; Expand Label expanded all 12; saved again | Canonical chemical graphs before/after ChemDraw expansion are identical                   |
| All 29 predefined groups                             | ReShiki text/group insertion                                                                                     | CDXML and CDX round trips preserve labels, atom/bond counts and canonical chemical graphs |

The initial group board exposed stale hydrogen-label caches: bare N was written instead of NH and ChemDraw flagged it. Export now derives H counts from the prepared graph. The original writer differential test hydrates H counts independently with RDKit to test this corrected contract.

A flat arene contact passing exactly through a ring vertex produced ChemDraw's proximity warning, “This atom is very close to another atom or bond.” Moving the external atom away from that vertex resolved it; no tilt or warning suppression was needed. This is a geometry check, not proof of an oxidation-state assignment.

Computer Use also exposed fixed bond lengths snapping center-to-metal drags onto ring atoms in ReShiki. Attachment drags now retain their requested length, use the existing angle constraint, and exclude their own targets from snapping. A 12-direction regression checks extended drags and both directions of metal/point snapping. Undo/redo, native save/copy/delete, malformed IDs, distributed charges, long V3000 continuation records and public import/export routing are covered by Rust tests. Locked RDKit also accepted all four exported MOL cases with intact ALL/ANY endpoint lists.

## Defined Cp/Cp* groups and dummy figure output

Typing Boc in Automatic/Chemical abbreviation mode retains the full fragment;
Text label mode deliberately creates only a named wildcard. All 29 ordinary
presets have CDXML/CDX graph-identity tests. New Cp and Cp* presets retain real
cyclopentadienyl (C5H5−) and pentamethylcyclopentadienyl (C10H15−) rings, plus a
five-center anchor. The star in Cp* is part of the group name, never a dummy.
Metal formal charges are left as entered. Composition counts real atoms and
ligands, excluding attachment nodes; this does not validate a coordination
sphere or provide a complex identifier. Variable/undefined attachments do not
claim a unique formula.

Computer Use found that ChemDraw 26 discarded nested multi-center definitions
when saving a collapsed Fragment label. The negative fixture retains this
actual output; import must reject its missing chemistry. Editable CDXML/CDX
therefore expands haptic groups, while ReShiki/native and figure output keep
Cp/Cp* labels. ChemDraw-resaved expanded Cp2Fe and Cp*2Fe fixtures preserve
C10H10Fe and C20H30Fe. The final Cp* example passed Check Structure with “No
errors found” after rotating ring vertices away from the metal contact path.
No tilt or warning suppression is required.

Anonymous dummy handles are a separate canvas overlay. SVG, PNG, PDF and Office
clipboard images share the figure scene, so dummy junctions have no label gap.
Regression tests compare their rendered output to an unlabeled carbon junction,
while checking that native/editable copy retains the wildcard and its bonds.
ChemDraw requires a hidden `Unspecified` node with an asterisk sentinel to avoid
converting a textless Element=0 into carbon on save. Its own renderer may leave
label padding at such hidden points; ReShiki figure/Office output does not.
