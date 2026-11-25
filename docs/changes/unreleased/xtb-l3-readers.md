---
section: Changes
issues: []
---

- File → Open reads Gaussian logs and fchk files, ORCA outputs, Molden files,
  and Gaussian, ORCA and GAMESS inputs: the last geometry of the last job that
  ended normally opens as a 3D model, and the status bar names the job and how
  it ended. Calculated results are read but not shown yet.
- Opening an XYZ file with several frames says how many frames it has; the
  first is shown.
