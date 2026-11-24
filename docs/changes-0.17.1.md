# ReShiki 0.17.1

ReShiki 0.17.1 adds editable bends to compression tubing and expands the
connected equipment library with bellows valves, sample cylinders, union
crosses and flexible hoses. Straight tube ends snap into matching sockets,
with reducer offers when the diameters differ.

[Release downloads](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.17.1) ·
[Previous release](changes-0.17.md) ·
[Release validation](changes/release-0.17.1-validation.md)

## File compatibility

Drawings saved by 0.17.0 still open in 0.17.1. ReShiki saves each drawing at
the lowest document version needed by its features. Editable compression-tube
routes use version 41 and require 0.17.1. Older versions ask you to update.
Ordinary drawings still use version 30.

| Document version | Examples of features that require it                                                                                                   | Earliest release that opens it |
| ---------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------ |
| 30               | Ordinary drawings and imported structures without newer features                                                                       | 0.13.1                         |
| 31–34            | Custom arrow-label gaps, thermal ellipsoids, 3D model metadata and two-sided residues                                                  | 0.14.0                         |
| 35–37            | Arrow interior nodes, custom crossing clearance, enhanced stereo groups and additional radical states                                  | 0.15.0                         |
| 38–39            | Linked property captions, name lines and typed molecule names                                                                          | 0.16.0                         |
| 40               | Editable cycles, panels, callouts, tables, TLC plates, equipment, new graphic effects, compound numbers and per-caption decimal places | 0.17.0                         |
| 41               | Editable compression-tube routes with circular bends                                                                                   | 0.17.1                         |

The new sample cylinders, bellows valves, pilot adapter and braided metal hose
also need 0.17.1 because they use equipment-definition fields or connections that
0.17.0 does not read, even when the drawing carries version 40. Use 0.17.1 or
later when sharing drawings that contain these pieces.
[Document format](architecture.md#document-format-versions)

## Highlights

- **Editable tube bends.** Add a bend point to stainless compression tubing,
  then drag the free leg or enter its angle and centerline radius. The cut
  length updates with the route. [PR #1408](https://github.com/Ameyanagi/ReShiki/pull/1408)
- **Tube-end snapping.** Drag either free end of a straight tube into a
  matching socket. Properties offers a reducer for a different diameter.
  [PR #1403](https://github.com/Ameyanagi/ReShiki/pull/1403)
- **Air-actuated valves and sample cylinders.** Add normally closed bellows
  valves with separate pilot connections, or single-ended and double-ended
  cylinders in 150 mL and 300 mL sizes. Exterior and Section retain their
  connections. [PR #1406](https://github.com/Ameyanagi/ReShiki/pull/1406) ·
  [PR #1404](https://github.com/Ameyanagi/ReShiki/pull/1404)
- **More tubing and hose connections.** Add four-way compression crosses,
  matching half-inch tubing, hose connectors and an editable braided metal
  hose. [PR #1405](https://github.com/Ameyanagi/ReShiki/pull/1405) ·
  [PR #1402](https://github.com/Ameyanagi/ReShiki/pull/1402) ·
  [PR #1407](https://github.com/Ameyanagi/ReShiki/pull/1407)

## Tubing and connections

- Right-click stainless compression tubing and choose **Add Bend Point…**.
  Drag the handle to turn the free leg around the point and release it to
  finish, or enter an angle and centerline radius in Properties and press
  Enter or leave the field. Each edit is one Undo step. Attached fittings
  keep their position and orientation. You can change the radius of an existing
  bend between fixed fittings when the new route fits. The equipment list
  reports bend count and developed cut length.
  [PR #1408](https://github.com/Ameyanagi/ReShiki/pull/1408)
- Drag either free end of an adjustable straight tube onto a matching
  compression socket to seat it at the internal stop, keeping the opposite
  end and connected fittings in place. When diameters differ, accept the
  matching reducer in Properties to insert and connect it in one Undo step,
  or place it manually when the current geometry prevents insertion.
  [PR #1403](https://github.com/Ameyanagi/ReShiki/pull/1403)
- Add four-way compression union crosses in 1/8, 1/4, 3/8 and 1/2 inch and
  6 mm. Each socket has its own tube stop, and Section shows all four
  passages. Matching adjustable 1/2-inch stainless tubing is included.
  [PR #1405](https://github.com/Ameyanagi/ReShiki/pull/1405)
- Add stainless hose connectors and matching 1/4-inch and 1/2-inch flexible
  hoses to the equipment library. A connector's compression adapter shank
  and hose barb have separate matching interfaces.
  [PR #1402](https://github.com/Ameyanagi/ReShiki/pull/1402)
- Add an editable FM-series stainless braided metal hose with 1/4-inch
  tube-adapter ends. Shape its route by dragging the middle and connect
  either end to a compression socket. The equipment parts list records its
  endpoints and catalog identity.
  [PR #1407](https://github.com/Ameyanagi/ReShiki/pull/1407)

## Valves and sample cylinders

- Add air-actuated, normally closed bellows shutoff valves for 1/4-inch,
  3/8-inch and 1/2-inch compression tubing. Each has a separate 1/8-inch
  female NPT pilot inlet. A matching compression-to-NPT adapter connects a
  pilot line in the drawing. Switch between **Closed, air off** and
  **Open, air on** to move the piston,
  return spring, bellows and soft valve tip. Exterior and Section keep the
  same process and air connections.
  [PR #1406](https://github.com/Ameyanagi/ReShiki/pull/1406)
- Add 150 mL and 300 mL stainless sample cylinders in single-ended and
  double-ended versions. Single-ended cylinders have a flat closed base;
  double-ended cylinders have an opening at both ends. All four offer
  Exterior and Section views with connectable 1/4-inch female NPT ports.
  [PR #1404](https://github.com/Ameyanagi/ReShiki/pull/1404)

## Known limits

- Equipment and equipment libraries remain preview features.
  [Editable equipment](graphics.md#place-lab-equipment)
- The braided metal hose is an editable schematic route. Its cut-length
  field is blank in the equipment parts list.
  [PR #1407](https://github.com/Ameyanagi/ReShiki/pull/1407)
- macOS downloads are signed and notarized. Windows and Linux downloads
  are unsigned. [Code signing policy](code-signing-policy.md)

## Build and verification

See the [0.17.1 release validation record](changes/release-0.17.1-validation.md).
