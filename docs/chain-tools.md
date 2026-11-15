# Chain drawing and bond constraints

Straight and snaking chain tools share the document, chemistry and export
pipeline with individually drawn bonds. This page covers how they behave; the
[first-molecule guide](https://reshiki.com/guide/first-molecule/#add-a-ring-or-chain)
shows how to draw them.

## Gestures

- A straight chain is one zigzag whose size follows the drag, or the **Atoms**
  count when one is set. A click with **Auto** places six atoms. Counts include
  existing attachment endpoints.
- A snaking chain follows pointer events at the preferred step size. Retracing
  an earlier vertex removes the tail; retracing to the start cancels.
  **Max atoms** caps the gesture and does not force a short drag to the full
  count. Ctrl during a straight-chain drag switches it to snaking.
- Starting on an atom extends its molecule. The final point reuses an existing
  atom only when that atom is close to it. Attachment atoms keep their IDs and
  elements. A click at a chain end continues the zigzag in its current
  direction.
- Overlapping an intermediate atom marks the preview red, and release leaves the
  document unchanged. Escape, focus loss and release outside the canvas also
  cancel.
- The whole chain is one history edit, so Undo and Redo remove or restore all its
  new atoms and bonds together.

The straight-chain icon is a four-carbon zigzag with 120° bends. The snaking
icon adds a steering arrow, which stands for pointer steering and retracing, not
a cis/trans conformation.

## Constraints and defaults

**Length** and **Angles** are independent. Bond direction snaps in 15°
increments and chain interiors default to 120°. Alt releases both constraints
for the current gesture. At an existing attachment atom, the atom's position
takes precedence over the nominal bond length.

Length accepts 1 to 300 pt and the chain angle 1 to 179°. With Length off, a
straight chain changes length continuously and a snaking chain can end with a
shorter step; with both constraints off, a straight chain's endpoint follows the
pointer. Very acute angles or crowded geometry can overlap; choose another
direction or a longer bond.

**Reset bonds** restores the document's bond length, a 120° chain angle and both
constraints. It changes tool settings, not the drawing. Each new document starts
from the JACS / ACS drawing and text defaults, even after another document used
custom ones. Tool settings are session state; saved geometry and formatting stay
with each object.

## Grouped molecules

New atoms join the groups of the molecule they extend. Joining separately
grouped molecules merges the overlapping groups and keeps their captions and
integral status. Properly nested groups stay nested where possible. Ordinary
bond edits and template growth use the same editing path. Native and CDXML
tests join two grouped, captioned fragments without splitting the result across
group boundaries.

## Limits

- Chains create carbon atoms joined by single bonds. Changing elements or bond
  orders and running Check are separate steps. Geometry checks do not validate
  valence.
- A gesture creates at most 512 chain atoms. Drawing does not auto-pan the
  view, route around obstacles or shorten a chain at an interior collision.
- Chain angle and length are tool settings, not a document style editor. They do
  not restyle existing molecules, rings or imported documents.

The original 2026-09-20 desktop and test checks are in
[the chain tools record](changes/chain-tools-20260920.md). The
[feature status](feature-status.md) page tracks the remaining gaps.
