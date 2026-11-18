---
section: Fixes
issues: []
---

- Distributing a whole reaction no longer leaves its molecules and `+` signs on
  top of each other. Distribute spaces the molecules, and each `+` of the
  reaction moves with the gap between the two molecules it separates, which
  widens to make room for it.
- Changing a dotted hydrogen bond to a real bond now works when one of its two
  molecules has a reaction role: the joined molecule keeps the role. It was
  refused with "Assign complete molecules to reaction roles".
