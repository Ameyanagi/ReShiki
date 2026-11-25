---
section: Fixes
issues: []
---

- CDX and CDXML files now mark each reaction the way ChemDraw does, so electronic lab notebooks, ChemDraw, RDKit and Indigo read them as reactions. Every arrow with a reaction gets a step that lists its reactants, products, plus signs, reagents and conditions, and atom maps. Reactions without roles set by hand use the roles Preview reaction shows.
- Opening a CDX or CDXML file keeps its reaction steps as reaction roles, so a ChemDraw file saved again from ReShiki keeps its reactions.
