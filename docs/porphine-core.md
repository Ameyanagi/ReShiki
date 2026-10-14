# Draw a porphine core

The porphine template places a free-base 21H,23H-porphine core with equal bond
lengths and both N–H labels pointing inward. You can also build the scaffold
yourself with the reference align/stretch controls.

## Use it

### Place the template

1. Open **Templates → Macrocycles → Porphine (21H,23H)**, or search for
   **porphyrin**.
2. Place it in empty space. The core uses the drawing's bond length and stays
   editable.

The outline has four regular five-membered rings, equal bond lengths and 120°
meso corners. Its two opposite N–H labels point inward, and they stay inward
after **Flip vertical**, **Flip horizontal** or a 90° rotation.

### Build the scaffold yourself

1. Start with one regular carbon five-ring and one meso arm.
2. Use **Reference: align / stretch** to set the arm to 14.4 pt along its own
   axis, then align the outer ring edge horizontally. Pin the common point.
3. Copy the original at 90°, 180° and 270° around that point with **Copy +
   rotate**.
4. Add the four links from each meso site to the next ring. This is still a
   carbon scaffold.
5. Assign the four inward nitrogens and the bond pattern. Set only opposite
   nitrogens to N–H, and check that the formula is C20H14N4.

### Add a meso substituent

1. Choose the **Benzene** template and its source atom with **Connect with a
   bond**, then attach it at an outward meso carbon.
2. Use **Stretch bond** to change only the bridge length. The macrocycle and the
   phenyl ring stay rigid. The result is C26H18N4.

## Limits

- The template is the free base only. Metal complexes and substituted
  porphyrins need their own checks.
- Ring bonds cannot be stretched alone.
- Building the scaffold by hand is a set of construction stages, not a measured
  number of clicks.

## Implementation notes

This is explicitly **21H,23H-porphine**, C20H14N4, with opposite N–H sites. The
graph agrees with
[official OPSIN's name parsing](https://opsin.ch.cam.ac.uk/opsin/21H%2C23H-porphine.json)
and the
[original ChemIDplus depositor record](https://pubchem.ncbi.nlm.nih.gov/substance/135024075).
The graph without specified macrocyclic double-bond geometry has InChIKey
`RKCAIXNGYQCCAL-UHFFFAOYSA-N`; the chosen planar depiction has
`RKCAIXNGYQCCAL-CEVVSZFKSA-N`. These are separate facts in the catalog. The
standardized PubChem CID 66868 represents another N–H placement and is not the
exact-tautomer reference for this drawing. Fourfold outline symmetry does not
make the N–H tautomer fourfold chemically symmetric.

Coordinates come from a regular-pentagon construction and exact quarter turns.
For bond length L, the five-ring radius is `L / (2 sin 36°)`; the common-center
distance is `R sin 72° + R cos 72° + sqrt(1.5) L`. At the default size the ring
center is 96.45835 drawing units from the common center. No reference-editor
artwork or coordinates are bundled. The regeneration script validates the
catalog's chemical graph and keeps this model-generated layout.

Fixed hydrogen positions (Left, Right, Above, Below) follow flips and
rotations of the selected atoms: each takes the nearest side after the
transform, and an exact 45° diagonal picks Left or Right. **Automatic** stays
automatic, and label text stays upright.

### Scaffold fixtures

The [user's twelve-stage guide](https://note.com/budhalocyanine/n/n7fe04cbb8f4e)
uses temporary helper polygons and guide lines to keep symmetry and bond
lengths. The construction fixtures instead demonstrate five scaffold stages.
The supplied seed has a 17.3° reference edge and a 21.6 pt arm, so it exercises
arbitrary angles, and the common point is pinned at X=0, Y=0. The supplied
fixtures demonstrate the algorithm through the application's geometry and
renderer. The [desktop record](changes/porphine-core.md) also covers
successive 90° copies, closure and N/bond assignment from that supplied seed;
from-blank step counts and timing comparisons were not measured. The template
is the direct placement route.

### Reproduce the figures

```sh
cargo run --locked --example porphine_core_qa -- /tmp/porphine-core
```

The generator writes editable `.rsk` files, actual SVG/PNG/PDF output, the
chosen template and constructed-core identity reports, seven
scaffold/substitution stages, and exact reproduction notes. Each image must be
inspected before publication. The [desktop record](changes/porphine-core.md)
covers template search and placement, shared-pivot copies, bridge stretching
and fresh reopening, with editable input/save files and independent chemistry
checks.
