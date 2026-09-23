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
