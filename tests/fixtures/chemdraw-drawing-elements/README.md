# ChemDraw drawing elements

Drawings that ChemDraw itself saved, with the shapes, lines, arrows, orbitals,
symbols, brackets and curves of its drawing tools in every style. They check
that a style ReShiki cannot draw yet does not stop the whole file from opening
(`crates/io/tests/chemdraw_drawing_elements.rs`).

| File                                           | Source                                                                                                                                                         |
| ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `drawing.cdx`, `drawing.cdxml`                 | The project owner's own test drawing of ChemDraw's basic shapes, saved by ChemDraw 26.0.0.6599 on macOS.                                                       |
| `p1`–`p6` `*.source.cdxml`                     | Sheets written for this project. Each lists every documented CDXML value of one family, one object per cell with a caption.                                    |
| `p1`–`p6` `*.chemdraw.cdx`, `*.chemdraw.cdxml` | The same sheets after ChemDraw 26.1.0.6801 opened and saved them again, so they hold what ChemDraw writes: single brackets, arcs as arrows, its binary values. |

The sheets are `p1` shapes, `p2` orbitals, `p3` symbols, `p4` brackets,
`p5` arrows and lines, and `p6` curves and arcs.

No file here comes from ChemDraw's bundled samples or templates, or from a
third party.
