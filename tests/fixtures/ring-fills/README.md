# ChemDraw native ring fill reference

Created on 2026-09-26 in ChemDraw 26.0.0.6599 on macOS. A cyclohexane
was drawn using ChemDraw's ring tool and filled with its enabled
`changeRingFillColor` command through the public AppleScript interface.
Although this installation identifies as Prime and hides the toolbar,
its scripting interface reports the command enabled and creates a native fill.

The Dark Mode stationery's unrelated page annotation was removed, the page
changed to white with black foreground, and ChemDraw saved these XML and binary
files. The ring color is RGB (29, 139, 39) in the saved files. No ReShiki exporter created the fill.

The fragment contains `ColoredMolecularArea` (binary object 0x8032), with a
`bgcolor` palette index and `BasisObjects` referencing its six bonds. It contains
no filled curve or image. These files test native ownership, not just appearance.

`chemdraw-return.cdx` is the actual clipboard data after copying the rebuilt
ReShiki caffeine example into ChemDraw, moving one ring atom, and copying it
back. It has 14 atoms, 15 bonds, two native filled areas, no curves or images,
and includes a ChemDraw stationery annotation (0x802b). Both fills are RGB
(67, 99, 132). Its bond-reference ordering differs from ReShiki's cycle order.
