# Chemical bracket export boundary

`rotated-semantic-bracket.rsk` is a valid native drawing derived from
`chemdraw-import-corpus/supported/041-polymer-repeat-1.cdxml`. Its chemical
bracket has local axes `(84, 84)` and `(-84, 84)`.

Before the export guard, CDXML/CDX export succeeded but encoded the opposite
corners as an axis-aligned `BoundingBox`. Both corners have the same x
coordinate, so reimport collapsed the 168-unit-wide bracket to a vertical
stroke. The chemical repeat definition remained present, making object-count
checks insufficient to detect the lost geometry.

Expected result: editable CDXML/CDX export (including a subsequent CDS
encoding) rejects the frame with advice to save as native ReShiki. Native
save and figure export retain the authored frame. The integration test
`transformed_semantic_bracket_frames_reject_editable_export_without_changing_the_source`
also checks skewed and mirrored frames for square, round and curly brackets.

`dashed-semantic-bracket.rsk` retains the original aligned bracket frame but
uses a dashed stroke. Before the appearance guard, editable export silently
returned it as solid. Dashed and dotted chemical brackets now reject editable
export with native-save advice; open bracket fills do not affect their paint.
`non_solid_semantic_brackets_reject_editable_export_without_changing_the_source`
checks both patterns and all three paired bracket kinds.
