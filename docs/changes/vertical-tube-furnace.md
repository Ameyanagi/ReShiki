# Upright furnace and direct compression end seals

The catalogue gains a distinct vertical furnace and two 32 mm work-tube end
seals with direct female compression connections. The existing horizontal
furnace, its nozzle-and-adapter option and the 600 mm quartz tube are unchanged.

The upright artwork uses Asahi Rika's
[ARF-40KC with vertical stand on the catalogue cover](https://www.kanjyouro.com/download/pdf/arf_1zone.pdf#page=1)
and [upright aluminium frame](https://www.kanjyouro.com/option/stand/) as visual
references. The control module, frame, split cover and side handle are original
native paths. The open centre leaves room for both tube ends throughout axial
travel.

The separate **Split tube furnace body, 32 mm work tube** retains only that
housing, vents, latches, handle and locating apertures. It adds one catalogue
asset without changing the supported upright or horizontal variants. A focused
model case rotates the same bare body, centred quartz tube, direct end seals
and tubing through the production transform path. It checks authoritative port
positions, all five seats, both views, save/reopen and the unchanged travel and
clearance in upright and horizontal orientations.

## Inputs and checks

- [¼-inch assembly](../../tests/fixtures/vertical-tube-furnace/connected-1-4.rsk)
  and [⅛-inch assembly](../../tests/fixtures/vertical-tube-furnace/connected-1-8.rsk)
  each contain six independently selectable equipment objects and five seats.
- The direct gas connections use the existing compression family with exact
  6.35 mm and 3.175 mm nominal ODs. Their insertion stops match the stock union
  definitions at 13.5 mm and 10.5 mm; the work-tube interface remains 32 mm with
  18 mm insertion.
- Focused model cases cover the vertical 300 mm bore, view-independent ports,
  full native save/reopen, both ±120 mm travel limits, 7 mm seal clearance,
  the open frame corridor, exact tubing matches and unblocked section passages.
- Lightweight geometry checks validate all three new definitions in Exterior
  and Section. Temporary baseline-compatible copies use the existing compression
  family in place of WorkTube only because the review executable predates that
  family; committed definitions and fixtures retain WorkTube. Native-renderer
  proofs include both sizes, both views, light/dark canvases and both travel
  extremes. These establish the artwork and coordinates; exact-code model
  execution and desktop captures use the coordinated build before PR review.

No solver, connector schema or existing equipment artwork changes are required.
The [furnace guide](../tube-furnace.md#upright-furnace-and-direct-compression-end-seals)
provides the assembly recipe and evidence-generation command.

Suggested release caption: **Upright furnace diagrams can connect directly to
¼-inch and ⅛-inch compression tubing, with the passage visible in Section.**
