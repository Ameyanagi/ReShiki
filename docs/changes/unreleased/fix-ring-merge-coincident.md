---
section: Fixes
issues: []
---

- **Rings join the atoms they land on.** A ring placed on an atom or bond now
  shares an existing atom under one of its other corners instead of stopping
  with “Overlaps an existing atom”, so a ring can be fused into the bay between
  neighbouring rings. The shared atom still needs a free valence for each bond
  the ring adds to it. A ring that is already drawn is refused with “The ring
  already exists”. See
  [Selection and ring placement](selection-and-ring-placement.md).
