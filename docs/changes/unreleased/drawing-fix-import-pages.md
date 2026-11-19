---
section: Fixes
issues: []
---

- ChemDraw files with many pages, such as the Bio Instruments, Anatomy,
  Animals, Hexoses and Amino Acids template libraries, now open. Their pages
  are placed in rows, in page order, so **Fit** shows the whole library instead
  of a thin column. Before, a long column of pages ran past ChemDraw's
  coordinate range or lost precision, and the file was refused.
- An abbreviation label with more than one font or size, such as "D-Trp" with
  a small D, opens in the style of its first character with a message, as
  element labels already did. Before, the file was refused.
- Older CDX files that keep atom labels straight in a group, such as the
  Anatomy template, and CDX files whose atom number or stereo labels ChemDraw
  tagged with a bare name now open.
- When a file cannot open, the status bar names it, a CDX file's error names
  CDX instead of CDXML, and an empty tab opened for the file closes again.
