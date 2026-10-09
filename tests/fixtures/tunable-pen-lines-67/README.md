# Tunable pen paths (#67)

`before.rsk` retains an open path with alternating bends and a closed asymmetric
outline. Each shape is one connected multi-segment object, beyond a single
circular or elliptical arc. The native model and exports already support these
commands at the declared #91 stack base `6da7348d` (production source
`477b97f4`); the general pen authoring tool and node insertion/deletion controls
are missing. Retained baseline model/IO observations were originally made at
`b8e524e0`; that existing Path geometry and point-editor behavior are unchanged
through the declared base.

Open the same fixture at a matched zoom, select the upper path and choose Edit
curve points. Baseline draws every node/control as a circle. Moving the interior
node (95, 30) moves that point alone, leaving its adjacent controls (95, -30) and
(95, 90) behind. Node edits should transport the adjacent controls together.

After implementation, create one path with several click/drag segments, inspect
the round nodes and square tangent controls, insert a node without reshaping its
segment, delete a node, convert a segment between straight/curved and close/open
the path. Closing adds a straight last-to-first segment; opening removes that closing segment. Capture the selected controls and a second finished drawing. Check
Escape, one Undo/Redo per gesture, transforms, duplication, native reopen and
editable CDX/CDXML. Arc presets remain available separately.
