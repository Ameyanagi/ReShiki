# Gas-line valves and instruments

The initial catalogue contains 14 native assets with original, generic exterior
and section illustrations. Manufacturer references establish the nominal
connections, topology and mechanical anatomy; the drawings are not dimension-exact
replicas of vendor products. They do not specify operating conditions or a relief
set point.

## Initial lineup

In the table, **C1/8** and **C1/4** mean compression sockets for tubing with an
outside diameter of **3.175 mm** and **6.35 mm**, respectively. **NPT F** and
**NPT M** mean **1/4 in female NPT** and **1/4 in male NPT**. Port names are stable
IDs. The source groups below provide the corresponding primary references.

| Asset ID                                  | Nominal reference                              | Port IDs and topology                                                             | Generic anatomy                                                     |
| ----------------------------------------- | ---------------------------------------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| `gas-ball-valve-1-8`                      | Swagelok SS-41GS2 [B]                          | `inlet`, `outlet`: opposed C1/8; bidirectional shutoff                            | Bored ball, vertical stem, quarter-turn lever                       |
| `gas-ball-valve-1-4`                      | Swagelok SS-42GS4 [B]                          | `inlet`, `outlet`: opposed C1/4; bidirectional shutoff                            | Bored ball, vertical stem, quarter-turn lever                       |
| `gas-needle-valve-1-8`                    | Swagelok SS-ORS2, letter O [N]                 | `inlet`, `outlet`: opposed C1/8; straight regulating pattern                      | Adjustment knob, threaded stem, tapered tip and seat                |
| `gas-needle-valve-1-4`                    | Swagelok SS-1RS4 [N]                           | `inlet`, `outlet`: opposed C1/4; straight regulating pattern                      | Adjustment knob, threaded stem, tapered tip and seat                |
| `gas-check-valve-1-8`                     | Swagelok SS-2C-1/3 [C]                         | `inlet` → `outlet`: opposed C1/8                                                  | Axial poppet, return spring and closing seat                        |
| `gas-check-valve-1-4`                     | Swagelok SS-4C-1/3 [C]                         | `inlet` → `outlet`: opposed C1/4                                                  | Axial poppet, return spring and closing seat                        |
| `gas-safety-relief-1-4-npt`               | Swagelok PRV2 [R]                              | Bottom `inlet` → perpendicular side `discharge`: both NPT F                       | Upright spring tower, poppet and seat; connectable discharge        |
| `gas-pressure-reducing-regulator-1-4-npt` | Swagelok KPR1FRA412A20000, A configuration [P] | `inlet` → `outlet`: two opposed NPT F; downstream pressure sensing                | Adjusting spring, diaphragm, regulating poppet and inlet filter     |
| `gas-back-pressure-regulator-1-4-npt`     | Swagelok KBP1F0A4A5A20000, A configuration [P] | `inlet` → `outlet`: two opposed NPT F; upstream pressure sensing                  | Adjusting spring, diaphragm and a distinct upstream seat passage    |
| `gas-pressure-gauge-1-4-npt`              | WIKA 232.50, NS63 / 2½ in, lower mount [I]     | One bottom `process`: NPT M; instrument terminal                                  | Dial, pointer and Bourdon tube mechanism                            |
| `gas-pressure-transducer-1-4-npt`         | WIKA A-10, male NPT option [I]                 | One bottom `process`: NPT M; instrument terminal                                  | Pressure-sensing end, electronics housing and electrical connector  |
| `gas-gauge-tee-1-4`                       | Swagelok SS-400-3-4TTF [F]                     | Opposed `left`, `right`: C1/4; perpendicular `instrument`: NPT F; all communicate | Hollow tee body, two compression nuts and threaded branch           |
| `gas-compression-npt-adapter-1-8`         | Swagelok SS-200-1-4 [F]                        | Opposed `compression`: C1/8 and `thread`: NPT M                                   | Open passage, compression nut, wrench flats and tapered male thread |
| `gas-compression-npt-adapter-1-4`         | Swagelok SS-400-1-4 [F]                        | Opposed `compression`: C1/4 and `thread`: NPT M                                   | Open passage, compression nut, wrench flats and tapered male thread |

## Views, state and attachment

**View** offers **Exterior** and **Section**, stored as `exterior` and `section`.
Ball-valve states are `closed` and `open`; needle-valve states are `closed`,
`part-open` and `open`. The check-valve section depicts passive spring-and-poppet
anatomy, without a manual actuator. The relief cutaway offers `seated` and
`relieving` illustration states. A transducer's electrical connector is not a
fluid port. A gauge or transducer terminates its instrument branch rather than
providing a through-flow outlet.

The selected KPR and KBP **A configurations each have only two opposed flow
ports**. Neither body gains a gauge, relief or vent connection from another
configuration. Their exterior forms may be similar, but they retain distinct
regulator kinds, sensing domains and internal passages. Their Exterior view looks
straight down the contoured adjustment knob, concentric bonnet and body; the
opposed flow ports remain in the drawing plane. Original PR and BP cap markings
identify pressure-reducing and back-pressure functions. Section retains the
longitudinal mechanism view and the same port seats. The separate gauge tee
provides the instrument branch.

Only **1/4 in NPT** is included initially; BSPP is not included. Its plug family
is `npt`; `size_mm: 6.35` encodes the nominal quarter-inch designation, while
**13.716 mm** is the illustrated thread rim/snap diameter. An NPT connection matches
only the same NPT nominal size with the opposite gender. It cannot match
compression merely because both records contain `6.35`: compression uses the
exact tubing outside diameter. The C1/8 and C1/4 sockets remain separate exact-OD
connections.

The PRV2 reference uses the manufacturer's order key: `PRVN2F-02-0-VV` is a
catalogue-derived configured reference for the selected nominal construction,
not a separately verified product listing or a complete operating selection.
The PRV catalogue identifies spring/poppet safety-relief anatomy and bottom-inlet,
side-discharge geometry. Swagelok R/RL proportional relief models are separate
families and are not the reference for this safety-relief asset. No 1/8 in
instrument tee or extra threaded sizes are inferred from the initial lineup.

## Primary references

- **[B] Ball valves:** Swagelok [40G/40 catalogue, pp. 3 and 6](https://www.swagelok.com/downloads/webcatalogs/en/MS-02-331.pdf), [SS-41GS2](https://products.swagelok.com/en/c/2-way-straight-pattern/p/SS-41GS2) and [SS-42GS4](https://products.swagelok.com/en/c/2-way-straight-pattern/p/SS-42GS4).
- **[N] Needle valves:** Swagelok [integral-bonnet catalogue, pp. 2–4](https://www.swagelok.com/downloads/webcatalogs/en/MS-01-164.pdf), [SS-ORS2](https://products.swagelok.com/en/c/straight-pattern-regulating-stem/p/SS-ORS2) and [SS-1RS4](https://products.swagelok.com/en/c/straight-pattern-regulating-stem/p/SS-1RS4).
- **[C] Check valves:** Swagelok [C-series catalogue, pp. 2 and 11](https://www.swagelok.com/downloads/webcatalogs/en/MS-01-176.pdf), [SS-2C-1/3](https://products.swagelok.com/en/c/fixed-pressure/p/SS-2C-1%252F3) and [SS-4C-1/3](https://products.swagelok.com/en/c/fixed-pressure/p/SS-4C-1%252F3).
- **[R] Safety relief:** Swagelok [PRV catalogue, pp. 2 and 6](https://www.swagelok.com/downloads/webcatalogs/en/MS-02-432.pdf), including the anatomy, connection table and ordering key.
- **[P] Regulators:** Swagelok [K-series catalogue, pp. 6–7, 24–25 and 52](https://www.swagelok.com/downloads/webcatalogs/en/ms-02-230.pdf), [KPR1FRA412A20000](https://products.swagelok.com/en/c/single-stage/p/KPR1FRA412A20000) and [KBP1F0A4A5A20000](https://products.swagelok.com/en/c/back-pressure-regulators/p/KBP1F0A4A5A20000). Page 52 establishes A-configuration port topology.
- **[I] Instruments:** WIKA [232.50/233.50 model](https://www.wika.com/en-us/232_50_233_50.WIKA) and [PM 02.02, pp. 2, 5 and 8](https://www.wika.com/media/Data-sheets/Pressure/Pressure-gauges/ds_pm0202_en_co.pdf); WIKA [A-10 model](https://www.wika.com/en-us/a_10.WIKA) and [PE 81.60, p. 5](https://www.wika.com/media/Data-sheets/Pressure/Pressure-sensors/ds_pe8160_en_co.pdf). The gauge's NS63 lower-mount 1/4 NPT combination is explicit; the transmitter's ordinary NPT option is distinct from its female-thread option.
- **[F] Fittings:** Swagelok [SS-400-3-4TTF](https://products.swagelok.com/en/c/tees/p/SS-400-3-4TTF), [SS-200-1-4](https://products.swagelok.com/en/c/straights/p/SS-200-1-4) and [SS-400-1-4](https://products.swagelok.com/en/c/straights/p/SS-400-1-4).
