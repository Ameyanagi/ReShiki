---
section: Changes
issues: []
---

- Ring, template and bond previews and moves of part of a structure take less
  work in large drawings. When the structures a preview changes stay clear of
  the rest of the drawing, the preview redraws only those structures and
  reuses the prepared drawing of the rest.
- Panning a zoomed-out drawing no longer redraws every bond and label on each
  frame. The first pan at a zoom level prepares the drawing a little beyond
  the window's edges, and later pans move that drawing.
