# Bond tools and exchange

Added 2026-09-20. The bond palette and contextual menu now offer **17 presets**. Select a bond to change its preset, RGB color and double-line position in Properties. The drawing canvas, SVG, PDF and PNG use the same geometry.

| Preset                                            | Meaning in Moruno                                                              |
| ------------------------------------------------- | ------------------------------------------------------------------------------ |
| Single, double, triple                            | Ordinary covalent orders; existing midpoint click cycling remains available    |
| Solid wedge, hashed wedge                         | Tetrahedral up/down depiction from the first endpoint                          |
| Hollow wedge, parallel hashed                     | Alternate up/down depictions                                                   |
| Bold                                              | Up depiction for a single bond; thick stroke                                   |
| Wavy                                              | Unspecified single-bond stereochemistry                                        |
| Coordination (dashed), dative                     | Directed donor-to-acceptor RDKit dative bond; dashed line or arrow             |
| Hydrogen bond                                     | Noncovalent interaction from an explicit bonded H to an acceptor               |
| Partial (solid / dashed), partial (double dashed) | Fractional 1.5 order and the indicated line pattern; see semantic limits below |
| Bold double                                       | Bold primary line and ordinary secondary line                                  |
| Crossed double                                    | Double bond with unspecified E/Z stereochemistry                               |
| Quadruple                                         | Four-line bond and RDKit quadruple order; shortcut `4`                         |

## Workflow

- Choose a palette tool, then use its contextual preset menu for the full set. Drag to see the actual bond style before release. Existing carbon-chain growth and attachment still work.
- Click an existing bond to apply a style. Repeated clicks with a matching solid/hashed/hollow wedge, parallel hash, bold, dative or coordination tool reverse its endpoints. Single/double/triple tools retain their order cycle.
- With Select (`V`), click a bond's middle to select both endpoint atoms. Properties applies bond edits to edges whose two endpoints are selected. A larger selection can style multiple bonds together; mixed presets are indicated.
- Double and partial bonds offer Automatic, Centered, Left and Right second-line placement. Reversing an edge or reflecting the drawing adjusts its relative side.
- Enter a hex color and press Enter or Color. Colors and line placement do not change molecular identity. Style/order changes invalidate chemistry so Check recomputes it.
- Hydrogen bonds must start at an existing explicit H with a covalent bond to another atom, and end at N, O, F or S with nonpositive formal charge. Unsupported endpoints leave the drawing unchanged. The interaction does not increase the hydrogen atom's valence.
- Drawing, restyling and direction changes are undoable. Save `.moruno` to retain the complete state.

## Native and chemical representation

Native document version **6** adds `double_position`, `secondary_display` and per-bond RGB `color`, with defaults for older files. The app and worker continue to accept native versions 1–5. Existing order 4 means aromatic; new order values are 0 hydrogen, 5 dative, 6 quadruple and 7 fractional 1.5. These numeric values belong to Moruno's file format, not chemical bond-order notation.

Cleanup preserves bond appearance and restates visible tetrahedral stereochemistry against the new coordinates. Tests remove stored atom stereochemistry and infer identity from the resulting drawing independently. Bold/hollow up depictions can become parallel hashes when a reflected or regenerated projection requires a down depiction; exact stylistic restoration is not guaranteed by two reflections. Undo restores the original style.

## CDXML exchange

The public SDK describes [bond display](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Bond_Display.htm), [secondary display](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Bond_Display2.htm), [double-line position](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Bond_DoublePosition.htm) and [order](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Bond_Order.htm). Interchange behavior is verified with saved fixtures and regression tests.

Live file exchange exposed encoding details absent from the older SDK:

- The supported external hydrogen-bond encoding is `Display="Dash" Display2="DottedHydrogen"`, with ordinary/default order. Moruno restores its noncovalent meaning and emits that encoding.
- Dashed coordination saves as an ordinary/default order plus `Display="Dash"`; Moruno imports it as directed dative chemistry.
- Double-dashed partial bonds need an explicit second `Dash` display when exported.
- Import supplies the ordinary secondary display when a bold double bond omits it.

A saved interchange gallery under `tests/fixtures/` contains 18 bonds. These cover all 17 presets plus the donor–H covalent bond. Tests assert styles, chemical orders, selected line patterns and hydrogen valence after import. Atom association must be unambiguous; an incomplete or ambiguous fragment parse cancels import rather than attaching styles to the wrong atoms.

## Verification and limits

**74 Rust tests and 23 Python tests pass**, alongside formatting and Clippy. Added coverage includes all presets through native and CDXML round trips; cleanup; visible tetrahedral identity; reflection; manual line position; SVG/PNG colors; PDF generation; selected-bond history; hydrogen valence; dative MOL round trips; and rejection of lossy molecular exports.

Desktop checks drew and reversed a hollow wedge, changed a selected double bond's side and color, exercised Undo/Redo, and saved native version 6. The interchange gallery was visually inspected after save/reopen. Local QA files are under ignored `artifacts/bond-qa-20260920/`.

The rebuilt standalone bundle passed signature verification and its engine check from `/tmp` with `MORUNO_ROOT=/nonexistent`. Its bundled worker imported the interchange fixture as 35 atoms and 18 bonds with formula `C26H75Cu2N3ORe2+4`. The packaged desktop app opened that file, saved it as native version 6, reopened the styled native review drawing and successfully checked its chemistry. The review drawing includes one additional hollow-wedge bond; its long formula now fits the Properties panel, and the saved red color/right-side double-line controls reappear on selection.

Remaining boundaries:

- Moruno currently stores these as fractional 1.5 bonds. It does **not** implement the full tautomer/query or delocalized-resonance behavior associated with those tools. The ordinary aromatic-ring workflow remains separate.
- Hydrogen endpoint validation is a local heuristic, not comprehensive hydrogen-bond perception. It does not create an explicit H automatically or assess geometry/interaction energy.
- SMILES export rejects hydrogen and fractional bonds; InChI also rejects dative and quadruple bonds. MOL export rejects hydrogen, fractional and quadruple bonds. Dative MOL exchange is tested. Native and drawing exports retain the supported styles; molecular formats do not retain all drawing appearance.
- Query/multiorder bonds, multicenter attachments, full metal/non-tetrahedral stereochemistry and perspective tools remain incomplete or absent. The subsequent [chain tools update](chain-tools.md) adds straight/snaking drags and fixed-angle/length controls.
- Unusual aromatic systems, dense metal complexes, source-specific CDXML attributes and broad chemical equivalence require additional differential fixtures. The gallery is a feature sample, not exhaustive chemical validation.

See the broader [feature-gap audit](feature-status.md) for the remaining workspace and chemistry features.

The Style palette now offers **All selected**, **Text**, and **Bonds** color scopes. Choose Bonds to recolor selected bond strokes while retaining label colors, or All selected to recolor the complete selected drawing. Click a bond’s middle to select both endpoints; Shift-click adds more. The selection summary explicitly counts selected bonds.

Clicking an existing double bond with the matching double-bond tool cycles centered, left and right placement. Its order, chemical identity and color stay unchanged. Hovering or selecting a bond supports S/D/T for single/double/triple; repeated D also cycles line position. Each change is one Undo step.
