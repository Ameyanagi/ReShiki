---
section: Changes
issues: []
---

- The Calculate card runs Opt + freq (a tight final optimization, then
  frequencies) and Frequencies with GFN2-xTB, g-xTB or GFN-FF, for molecules
  of up to 200 atoms.
- The xTB results palette gains a Vibrations tab: the modes with their IR
  intensities (imaginary modes first, in red), the selected mode animated on
  the 3D model with play/pause, amplitude, speed and displacement arrows that
  can be added to the drawing, a broadened IR spectrum (Lorentzian or
  Gaussian, FWHM and scale factor), Export mode as XYZ movie, and
  thermochemistry (ZPVE, H, S, G, Cp, point group) at any temperature and
  pressure with xtb's quasi-RRHO defaults.
- A result with an imaginary frequency says it is not a minimum and offers
  Push along mode and re-optimize; only a converged re-optimization replaces
  the structure.
- Saved files keep frequencies, intensities, masses and thermochemistry; the
  normal modes are recomputed when the Vibrations tab opens.
