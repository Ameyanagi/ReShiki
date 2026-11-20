---
section: Fixes
issues: []
---

- Titles, legends and labels that ChemDraw draws on shaded or shadowed boxes,
  as in the Citric Acid Cycle and Cyclic Peptide samples, stay readable when
  the file opens. Before, the boxes covered them. A shape now opens in front of
  the chemistry only when ChemDraw stacks it above every structure, text and
  arrow.
- Lines and arcs in ChemDraw files open as graphic lines and curves, like
  a line drawn in ReShiki, so **Properties** shows **Graphic style** and you
  can add an arrowhead. Before, they opened as reaction arrows with the
  **Chemistry** purpose. Arrows with heads, and lines a ChemDraw reaction step
  lists, still take part in reactions.
- A line without heads is written to CDXML and CDX as ChemDraw's line, so it
  opens again as a line instead of a curve.
