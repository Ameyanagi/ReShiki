---
section: Changes
issues: []
---

- Export input file names where the coordinates come from, such as "xTB
  conformer 1 of 4 · GFN2-xTB", lets you pick another conformer or the
  optimized or start geometry, and writes the source into the file's title
  line. A geometry calculated as a triplet keeps its multiplicity.
- Switching the input file to GAMESS picks the nearest built-in basis set with
  a note, ORCA and GAMESS inputs get their own default file names, and the
  dialog stays open after Save so several programs take one trip. The command
  line and agent tools use the same defaults.
