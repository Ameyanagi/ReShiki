---
section: Fixes
issues: []
---

- **Clean up keeps large rings round.** A ring of nine or more atoms that
  shares no atom with another ring, such as cyclododecane or 18-crown-6, is
  cleaned up as a regular polygon with its substituents pointing outward, as
  in ChemDraw. It was folded into a lobed shape before. A trans double bond in
  the ring keeps its inward corner. Fused, spiro and bridged large rings are
  unchanged. See
  [Selection cleanup](selection-cleanup.md).
