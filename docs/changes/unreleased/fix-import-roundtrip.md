---
section: Fixes
issues: []
---

- A CDXML file ReShiki writes opens again with its panels and its Reference and
  Illustration purposes, so a reference compound no longer joins the reaction
  after the round trip. ChemDraw has neither, and CDXML and CDX export now say
  so. See [Object purpose](reactions.md#object-purpose).
- CDXML and CDX export write a panel's shadow, and the shadow of a rotated
  rectangle or a path, as a grey shape behind it. An unfilled shadowed rectangle
  or oval on a colored ChemDraw page, such as a CDXML file exported from the
  dark canvas, opens with the page color inside, as ChemDraw draws it, instead
  of a white face. See [Editable graphics](graphics.md).
