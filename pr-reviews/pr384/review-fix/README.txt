PR #384 Greptile thread PRRT_kwDOUd81186uPvOr - fix 2668da34
Scenario: glucose C4 HO -> OBn, set alignment Above/Center/Left/Right, retype ONp (no reverse spelling).
Before (PR head 1c4a2f72): placement dropped to Auto -> model-before.txt, app-before.txt
  left: ("ONp", Auto)  right: ("ONp", Above)  -> FAILED
After (2668da34): Above/Center/Left kept, Right still drops to Auto; O stays on the bond (anchor 0..1)
  -> model-after.txt, app-after.txt  -> ok
