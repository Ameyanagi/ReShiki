---
section: Fixes
issues: []
---

- 3D models and force-field optimization no longer give up on two kinds of large nano-material fragment: highly symmetric ones, such as the graphene flake C96H24, which now generates in about 2 seconds instead of timing out, and large flexible ones, such as a 150-atom dendrimer, whose usual starting geometry always failed and which now start from random coordinates.
