---
section: Changes
issues: []
---

- **Large drawings of many separate structures stay responsive.** Redrawing
  after an edit, measuring a selection and **Align** or **Distribute** now take
  time in step with the number of structures, labels, bonds and captioned
  arrows, whether they sit side by side or one above the other. In a drawing
  of 7,728 atoms, **Align left** on everything is about 20 times faster,
  measuring the whole selection about five times faster and building the
  picture about twice as fast. Small drawings are unchanged.
- One molecule whose bonds cross each other many times, such as a long peptide
  imported from SMILES, redraws about a quarter faster. Working out each
  crossing still takes most of that time.
- A drawing with an abbreviation that has two attachment points, such as an
  amino-acid residue in a chain, is no longer copied several times for every
  pointer move.
- When **Align** or **Distribute** moves an arrow that is attached to an atom
  or bond in another aligned object, the arrow moves as drawn and that end
  detaches. Before, the result depended on which object was farther left or
  higher.
