# Bond tools and exchange

Added 2026-09-20. The bond palette and contextual menu now offer **17 presets**. Single, double and triple bonds have direct toolbar buttons. Other styles open from the separate bond palette. Select a bond to change its preset, RGB color and double-line position in Properties. The drawing canvas, SVG, PDF and PNG use the same geometry.

| Preset                                            | Meaning in ReShiki                                                             |
| ------------------------------------------------- | ------------------------------------------------------------------------------ |
| Single, double, triple                            | Ordinary covalent orders; existing midpoint click cycling remains available    |
| Solid wedge, hashed wedge                         | Tetrahedral up/down depiction from the first endpoint                          |
| Hollow wedge, parallel hashed                     | Alternate up/down depictions                                                   |
| Bold                                              | Up depiction for a single bond; thick stroke                                   |
| Wavy                                              | Unspecified single-bond stereochemistry                                        |
| Coordination (dashed), dative                     | Directed donor-to-acceptor dative bond; dashed line or arrow                   |
| Hydrogen bond                                     | Noncovalent interaction from an explicit bonded H to an acceptor               |
| Partial (solid / dashed), partial (double dashed) | Fractional 1.5 order and the indicated line pattern; see semantic limits below |
| Bold double                                       | Bold primary line and ordinary secondary line                                  |
| Crossed double                                    | Double bond with unspecified E/Z stereochemistry                               |
| Quadruple                                         | Four-line bond and quadruple order; shortcut `4`                               |

## Workflow

- Choose a palette tool, then use its contextual preset menu for the full set. Drag to see the actual bond style before release. Existing carbon-chain growth and attachment still work.
- Click an existing bond to apply a style. Repeated clicks with a matching solid/hashed/hollow wedge, parallel hash, bold, dative or coordination tool reverse its endpoints. Ordinary single/double/triple bonds retain their order cycle; aromatic edges follow the appearance rules below.
- On the first conversion of a plain single bond to a solid, hashed or hollow wedge, the narrow end starts at the sole plausible stereocenter: an endpoint without multiple bonds that can have four substituents (including attached or implicit H, with a simple P/S extension), or a nonterminal endpoint opposite a terminal atom. If both endpoints or neither qualify, the existing direction stays. Clicking the matching wedge again still reverses it. Aromatic, dative and retained projection bonds keep their existing direction rules; drag previews use the same endpoint choice as the committed edit.
- With Select (`V`), click a bond's middle to select both endpoint atoms. Properties applies bond edits to edges whose two endpoints are selected. A larger selection can style multiple bonds together; mixed presets are indicated.
- Double and partial bonds offer Automatic, Centered, Left and Right second-line placement. For explicit Left/Right placement, reversing an edge or reflecting the drawing adjusts its relative side.
- Automatic double bonds point into their ring. A fusion double prefers the incident ring with more other double or aromatic bonds, then a six-membered ring on a tie. Substituted chain doubles offset toward an attached ring or, when no attached ring selects a side, the side with more substituent bonds. Equal counts then place the second line on the negative-cross-product side of the bond's stored start-to-end direction, using drawing coordinates with y increasing downward; CDXML retains its B-to-E direction. Unlabeled terminal carbons such as =CH₂ follow the substituent-side rule; terminal heteroatom doubles such as C=O remain centered. The chain tie and terminal-carbon rules follow ChemDraw 26.1's ACS Document 1996 style. **Center chain double bonds** in Document Style restores centered chain placement. Explicit positions remain available.
- Offset inner lines shorten at each adjoining angle bisector, with the established minimum setback retained for five- and six-membered rings. Small rings receive more shortening; an end without a neighbour on the offset side receives none. Drawing, crossings, arrow attachments and SVG/PNG/PDF exports share this geometry. Tilted 3D rings retain their existing face selection and polygon clipping.
- Enter a hex color and press Enter or Color. Colors, line placement and aromatic projection styles do not change molecular identity. Chemical order/stereochemical changes invalidate chemistry so Check recomputes it.
- Hydrogen bonds must start at an existing explicit H with a covalent bond to another atom, and end at N, O, F or S with nonpositive formal charge. Unsupported endpoints leave the drawing unchanged. The interaction does not increase the hydrogen atom's valence.
- Drawing, restyling and direction changes are undoable. Save `.rsk` to retain the complete state.

### Aromatic ring appearance

On an aromatic edge, solid/hashed/hollow wedges, bold, parallel hashes and wavy styles preserve the aromatic bond order. They depict the ring's projection without assigning tetrahedral stereochemistry. The aromatic circle or partial inner curve remains visible, including after a 3D tilt. Clicking the same wedge again reverses its direction; Single restores a plain edge while keeping the ring aromatic. Explicit Double/Triple and coordination presets still change the bond order.

This behavior is shared by bond clicks, drag previews, committed drags and Properties, with Undo/Redo. Native files and SVG/PNG/PDF retain the appearance. CDXML/CDX currently reject these projection styles explicitly; restore plain edges before editable exchange. Ordinary single-bond wedges retain their stereochemical meaning.

### Checking and redrawing stereochemistry

**Check drawing** reports stereobonds with no effect or a probably reversed narrow end, ambiguous wedge geometry, conflicting wedges, and undefined stereocentres or stereogenic double bonds. The notices select the affected atoms and bonds; clicking a notice in **Stereochemistry** selects that item again. **Reverse** fixes the probably reversed bonds in one Undo step. Explicitly unknown stereochemistry, such as a wavy bond, is excluded from the undefined count.

Enable **IUPAC style notes in Check drawing** in Stereochemistry for optional advice about bonds between stereocentres, mixed hash styles, alternate-only depictions, wavy bonds beside alkenes, re-entrant ring atoms and T-shaped geometry. This preference starts off and does not alter how bonds are interpreted.

**Redraw stereo bonds**, available in the selection context menu, command palette and stereo notices, lets the automatic wedge chooser place the wedges again. It acts on the molecules touched by the selection, or the whole drawing with no selection. It normalises solid/hollow/bold and hash depictions to solid or hashed wedges while retaining configuration, coordinates, colours and other bond properties. Wavy and projection bonds stay as drawn. The command is undoable.

Under **Show R/S and E/Z labels**, enable **Mark undefined stereocentres** to show muted **(?)** markers at undefined stereocentres and stereogenic double bonds. The document option starts off. Markers appear in figure exports only while visible; they are separate from chemical CIP labels. Generated-name notices list the specific units whose configurations are left unspecified.

## Native and chemical representation

Native document version **6** added `double_position`, `secondary_display` and per-bond RGB `color`, with defaults for older files. The app still accepts native versions 1–5. Order 4 means aromatic; version 6 added order values 0 hydrogen, 5 dative, 6 quadruple and 7 fractional 1.5. These numeric values belong to ReShiki's file format, not chemical bond-order notation.

Cleanup preserves bond appearance and restates visible tetrahedral stereochemistry against the new coordinates. Tests remove stored atom stereochemistry and infer identity from the resulting drawing independently. Bold/hollow up depictions can become parallel hashes when a reflected or regenerated projection requires a down depiction; exact stylistic restoration is not guaranteed by two reflections. Undo restores the original style.

## CDXML exchange

The public SDK describes [bond display](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Bond_Display.htm), [secondary display](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Bond_Display2.htm), [double-line position](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Bond_DoublePosition.htm) and [order](https://bobhanson.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Bond_Order.htm). Interchange behavior is verified with saved fixtures and regression tests.

Live file exchange exposed encoding details absent from the older SDK:

- The supported external hydrogen-bond encoding is `Display="Dash" Display2="DottedHydrogen"`, with ordinary/default order. ReShiki restores its noncovalent meaning and emits that encoding.
- Dashed coordination saves as an ordinary/default order plus `Display="Dash"`; ReShiki imports it as directed dative chemistry.
- Double-dashed partial bonds need an explicit second `Dash` display when exported.
- Import supplies the ordinary secondary display when a bold double bond omits it.

A saved interchange gallery under `tests/fixtures/` contains 18 bonds. These cover all 17 presets plus the donor–H covalent bond. Tests assert styles, chemical orders, selected line patterns and hydrogen valence after import. Atom association must be unambiguous; an incomplete or ambiguous fragment parse cancels import rather than attaching styles to the wrong atoms.

## Verification and limits

**74 Rust tests and 23 Python tests pass**, alongside formatting and Clippy. Added coverage includes all presets through native and CDXML round trips; cleanup; visible tetrahedral identity; reflection; manual line position; SVG/PNG colors; PDF generation; selected-bond history; hydrogen valence; dative MOL round trips; and rejection of lossy molecular exports.

Desktop checks drew and reversed a hollow wedge, changed a selected double bond's side and color, exercised Undo/Redo, and saved native version 6. The interchange gallery was visually inspected after save/reopen. Local QA files are under ignored `artifacts/bond-qa-20260920/`.

The rebuilt standalone bundle passed signature verification and its engine check from `/tmp` with `RESHIKI_ROOT=/nonexistent`. Its bundled worker imported the interchange fixture as 35 atoms and 18 bonds with formula `C26H75Cu2N3ORe2+4`. The packaged desktop app opened that file, saved it as native version 6, reopened the styled native review drawing and successfully checked its chemistry. The review drawing includes one additional hollow-wedge bond; its long formula now fits the Properties panel, and the saved red color/right-side double-line controls reappear on selection.

Remaining boundaries:

- ReShiki currently stores these as fractional 1.5 bonds. It does **not** implement the full tautomer/query or delocalized-resonance behavior associated with those tools. The ordinary aromatic-ring workflow remains separate.
- Hydrogen endpoint validation is a local heuristic, not comprehensive hydrogen-bond perception. It does not create an explicit H automatically or assess geometry/interaction energy.
- SMILES export rejects hydrogen and fractional bonds; InChI also rejects dative and quadruple bonds. MOL export rejects hydrogen, fractional and quadruple bonds. Dative MOL exchange is tested. Native and drawing exports retain the supported styles; molecular formats do not retain all drawing appearance.
- Query/multiorder bonds, multicenter attachments, full metal/non-tetrahedral stereochemistry and perspective tools remain incomplete or absent. The subsequent [chain tools update](chain-tools.md) adds straight/snaking drags and fixed-angle/length controls.
- Unusual aromatic systems, dense metal complexes, source-specific CDXML attributes and broad chemical equivalence require additional differential fixtures. The gallery is a feature sample, not exhaustive chemical validation.

See the broader [feature-gap audit](feature-status.md) for the remaining workspace and chemistry features.

The Style palette now offers **All selected**, **Text**, and **Bonds** color scopes. Choose Bonds to recolor selected bond strokes while retaining label colors, or All selected to recolor the complete selected drawing. Click a bond’s middle to select both endpoints; Shift-click adds more. The selection summary explicitly counts selected bonds.

Clicking an existing double bond with the matching double-bond tool cycles centered, left and right placement. Its order, chemical identity and color stay unchanged. Hovering or selecting a bond supports 1/2/3 for single/double/triple; repeated 2 also cycles line position. Each change is one Undo step.
