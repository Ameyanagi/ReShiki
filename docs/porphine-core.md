# Draw a porphine core

Open **Templates → Macrocycles → Porphine (21H,23H)**, or search for **porphyrin**.
Place the free base once. The outline has four regular five-rings, equal bond
lengths and 120° meso corners. Its two opposite N–H labels point inward. Placement
uses the drawing's bond-length setting; the core remains editable.

This is explicitly **21H,23H-porphine**, C20H14N4, with opposite N–H sites.
The graph agrees with [official OPSIN's name parsing](https://opsin.ch.cam.ac.uk/opsin/21H%2C23H-porphine.json)
and the [original ChemIDplus depositor record](https://pubchem.ncbi.nlm.nih.gov/substance/135024075).
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
catalog's chemical graph and retains this model-generated layout.

## Build the scaffold with reusable controls

The [user's twelve-stage guide](https://note.com/budhalocyanine/n/n7fe04cbb8f4e)
uses temporary helper polygons and guide lines to retain symmetry and bond
lengths. The construction fixtures instead demonstrate five scaffold stages:

1. Start with one regular carbon five-ring and one meso arm. The supplied seed
   has a 17.3° reference edge and a 21.6 pt arm, so it exercises arbitrary angles.
2. Use **Reference: align / stretch** to set the arm to 14.4 pt along its own
   axis, then align the outer ring edge horizontally. Pin the common point at
   X=0, Y=0 in the supplied fixture.
3. Copy the original seed at 90°, 180° and 270° around that same point.
4. Add the four links from each meso site to the next ring. This is still a
   carbon scaffold, not an asserted porphine.
5. Assign the four inward nitrogen sites and the validated bond phase. Set only
   opposite nitrogens to N–H; check the C20H14N4 identity before using the figure.

These are construction stages, not measured mouse/key counts. The supplied
fixtures demonstrate the algorithm through the application's geometry and
renderer; desktop interaction and time comparisons require a separate check.
The template is the direct placement route.

For a substitution example, choose Benzene's source atom and **Connect with a
bond**, then attach at an outward meso carbon. **Stretch** can change only this
bridge length while the macrocycle and phenyl ring remain rigid. The example has
formula C26H18N4. Rings cannot use single-edge rigid stretching. This change adds
one free-base core; metal complexes and derivative libraries remain separate
work requiring their own chemical validation.

## Reproduce the figures

```sh
cargo run --locked --example porphine_core_qa -- /tmp/porphine-core
```

The generator writes editable native files, actual SVG/PNG/PDF output, the chosen
template and constructed-core identity reports, seven scaffold/substitution
stages, and exact reproduction notes. Each image must be inspected before
publication, and real desktop evidence must separately check template search,
placement, shared pivots and bridge stretching.
