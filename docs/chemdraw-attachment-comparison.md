# ChemDraw attachment comparison

Checked ChemDraw 26.0.0.6599 on macOS on 2026-09-23. Computer Use successfully read the window and operated the Structure menu after its initial authentication error. Native macOS events were used for the floating bond palette and dragging, which Computer Use did not deliver reliably to this app. Only isolated test documents were edited.

| Operation                                                          | Observed result                                                                           | Meaning                                                                     |
| ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| Select six arene atoms → Structure → Add Multi-Center Attachment   | A central asterisk; CDXML `NodeType="MultiAttachment"`, `Attachments="10 11 12 13 14 15"` | One attachment representing the selected atoms together                     |
| Select three consecutive allyl atoms → Add Multi-Center Attachment | A point at their mean position; `MultiAttachment` references precisely the three atom IDs | The operation also works for an open chain, not only a ring                 |
| Select arene atoms → Add Variable Attachment                       | Central asterisk; CDXML `NodeType="VariableAttachment"`, with the candidate atom IDs      | Alternative attachment positions, not simultaneous multicentre coordination |

A bond can start at the multi-centre point and terminate in a metal label. The point is a dedicated node referencing existing atoms, not an extra carbon. Perspective is a separate operation: no tilt was needed to create either attachment type.

The vendor's [ChemDraw user guide, pages 171–173](https://kudos.kindai.ac.jp/cms/pdf/chemdraw_userguide.pdf#page=180) describes polyhapto drawings and positional-isomer notation separately. Arene/allyl haptic coordination needs multi-centre attachment; variable attachment represents the alternative-position case.

ReShiki currently has a tracked **drawing centroid** and wildcard atoms. Those are useful drawing controls, but are not equivalent to these two chemically meaningful attachment types. Its `E` variable label is also a label on a wildcard, not a variable attachment. Molecular export of centroid diagrams remains blocked to avoid silently turning haptic contacts into ordinary bonds.

Full compatibility would require distinct attachment kinds, retained member IDs, bonds terminating at those attachment nodes, and checked CDXML/MOL interchange. Supporting only their visible strokes would be insufficient.

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

Current gap: ReShiki's low-level CDXML reader follows the native reader's inert-node behavior, and its V3000 reader currently discards ENDPTS/ATTACH metadata. These code paths need typed preservation and boundary regression tests before attachment interchange can be advertised. This investigation does not claim that compatibility has been implemented. Plain SMILES and figure exports are not replacements for these semantics.
