---
section: Changes
issues: []
---

- Inspector → Predict has an NMR · xTB switch. The xTB page's Calculate card runs a GFN2-xTB energy or geometry optimization on the selected molecule (up to 500 atoms), with charge and unpaired electrons taken from the drawing, an optional ALPB solvent, and atoms held fixed on request. It makes the 3D model with MMFF first when needed, shows the step, energy change and time while the model moves, and Cancel or Esc applies nothing. An optimized geometry is applied to the 3D model as one Undo step; if the stereo check finds a changed stereocentre or double bond, the card asks first. Desktop builds with xTB only (macOS and Linux).
