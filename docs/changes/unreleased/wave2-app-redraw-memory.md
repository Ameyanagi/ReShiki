---
section: Changes
issues: []
---

- Large drawings stay responsive with everything selected. With the
  **Properties** tab open, ReShiki built each frame of a 7,700-atom drawing in
  about 580 ms after **Select all**. It now takes about 20 ms. A drawing with
  attachment points also builds its frame faster with nothing selected: 6 ms
  instead of 21 ms. The times here are medians on an Apple M1.
- The **Templates** tab draws each thumbnail once and keeps it. A frame with the
  largest collection open takes about 2 ms instead of 44 ms. The preview of a
  ring tool in the **Properties** tab is kept the same way.
- An unchanged large drawing costs less to redraw: about 3 ms instead of 14 ms
  for a 7,700-atom drawing, whether or not it is in view.
- Undo history has one memory budget for all tabs, about 256 MB, where each tab
  had its own 128 MB. When an edit goes over the budget, the tabs behind the
  front one give up their oldest Undo steps first and keep at least 5. The front
  tab keeps at least 20, as before. Five tabs with a 7,700-atom drawing each now
  hold about 320 MB of history instead of 700 MB.
