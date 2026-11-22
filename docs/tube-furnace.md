# Tube furnace pieces

The horizontal furnace, quartz work tube, two terminating end seals and two
gas-port adapters are separate equipment objects. The connected example in
[connected-exterior.rsk](../tests/fixtures/tube-furnace/connected-exterior.rsk) adds two
existing 120 mm, ¼-inch compression tubes: eight objects and seven seats.
Select **View → Section** on the pieces to expose the continuous passage.
Views change artwork without changing ports, poses or the parts represented.

## Connections

Geometry uses millimetres, with the quartz tube and furnace centred at the
origin and the tube axis along x. The furnace includes 32 mm locating
apertures; its casing is an original schematic drawing rather than the
specified envelope of a commercial model.

| Piece                            | Port            | Interface                          | Geometry                               |
| -------------------------------- | --------------- | ---------------------------------- | -------------------------------------- |
| `tube-furnace-32`                | `bore`          | Mechanical `through_bore`, 32 mm   | x = −150 to +150, y = 0                |
| `quartz-work-tube-600-32`        | `span`          | Mechanical `tube_span`, 32 mm      | x = −270 to +270, y = 0                |
| Work tube                        | `left`, `right` | Male `work_tube`, exact 32 mm OD   | x = −300 / +300, outward along −x / +x |
| `work-tube-end-seal-32`          | `work-tube`     | Female `work_tube`, exact 32 mm OD | x = 0, outward −x, insertion 18 mm     |
| End seal                         | `gas-nozzle`    | Male `compression`, 6.35 mm OD     | x = 54, outward +x                     |
| `work-tube-gas-port-adapter-1-4` | `nozzle`        | Female `compression`, 6.35 mm OD   | x = 0, outward −x, insertion 12 mm     |
| Gas-port adapter                 | `gas-line`      | Female `compression`, 6.35 mm OD   | x = 36, outward +x, insertion 12 mm    |

For a centred assembly, connect `span` to `bore`. Connect a seal's
`work-tube` socket to each tube end. Connect each adapter's `nozzle` socket to
its seal's `gas-nozzle`, then a compression tube end to the adapter's
`gas-line`. The left seal, adapter and outer tube turn 180° from their
canonical right-facing artwork.

The assembled origins in furnace millimetres are work tube 0; end seals
±282; adapters ±324; and the `a` ends of the outer compression tubes ±348.
The 600 mm work tube remains one piece through the full furnace. The bore
relation lets the tube and its end accessories move along the furnace axis
while the grounded furnace stays put. Drag the furnace casing or base to
move the whole connected assembly freely; drag the exposed tube to slide it
and its attached end accessories through the stationary furnace.
The authored mechanical span reserves
30 mm at each end of the 600 mm tube, leaving 540 mm available to the
300 mm furnace bore. Centred axial travel is therefore limited to ±120 mm.
The tube artwork and its end ports remain at ±300 mm.

Each seal's inner lip starts 18 mm inboard of its tube end, so the nearest
seal lip is at ±162 mm at a travel limit. The furnace's locating apertures
reach ±155 mm at the seal's height, leaving a 7 mm gap between material
boundaries in both views. This reserve comprises 18 mm for the seal,
5 mm for the aperture beyond the nominal bore end and 7 mm of clearance.
At the default 0.4 pt/mm scale, that leaves about 2.2 pt of clear space after
the two 0.6 pt outlines. The wider controller base sits below the seals.
This authored restriction covers this tube and seal set; the generic
relation does not perform collision detection for other accessories.

`work_tube` is distinct from `compression`, `hose` and other plug families.
Its nominal OD matches exactly: a 31.99 mm work-tube port does not match a
32 mm port. Insertion depth belongs only to female work-tube or compression
sockets. The seal's small tube nozzle uses the existing compression family,
so the separate adapter needs no invented threaded interface. A 6 mm tube
does not match a 6.35 mm (¼-inch) gas-line socket.

## Artwork references

The native artwork is original. The housing uses Asahi Rika's ARF-KC as a
visual reference for the split cover, arched lifting handle, vertical vent
slots and visible toggle latches. Its proportions and control module remain
generic. The independent work tube, terminating seal sleeve and gas nozzle
use the work-tube accessory reference:

- [Asahi Rika ARF-KC tube furnaces](https://www.kanjyouro.com/tube/arf/arf_kc/)
- [Asahi Rika ARF-50KC housing drawing](https://www.kanjyouro.com/download/pdf/arf-50kc.pdf)
- [Carbolite work-tube accessories](https://www.carbolite.com/products/modified-atmosphere/work-tube-accessories/)

The accessory catalogue includes end seals for 32 mm OD work tubes. The
600 mm length, 28 mm illustrated bore and 18 mm seal insertion are explicit
geometry choices for this diagram set. The pieces carry no pressure or
temperature operating rating.

Run the focused model checks and optionally regenerate native review files:

```sh
RESHIKI_TUBE_FURNACE_EVIDENCE=/absolute/review/directory \
  cargo test -p reshiki-model tube_furnace
```

The evidence output contains Exterior and Section `.rsk` and `.svg` files
on light and dark canvases, all generated from the tested native assembly.
