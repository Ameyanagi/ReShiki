# Default drawing style: JACS / ACS

Moruno defaults to the ACS structure preset for JACS-oriented drawings. This applies to canvas drawings, annotations, SVG, PDF, PNG, and the supported CDXML exchange subset. The application controls keep their own interface theme.

| Setting | Default |
| --- | --- |
| Drawing color | Black |
| Atom and annotation font | Arial, 10 pt |
| Bond length | 14.4 pt |
| Bond line width | 0.6 pt |
| Bold/wedge width | 2.0 pt |
| Multiple-bond spacing | 18% of nominal bond length |
| Label margin | 1.6 pt |
| Hash spacing | 2.5 pt |
| PNG resolution | 1200 dpi |

The numeric structure settings come from the [ACS graphics preparation guide](https://pubs.acs.org/paragonplus/submission/general/graphics_prep.html). The [current JACS author guidelines](https://researcher-resources.acs.org/publish/author_guidelines?coden=jacsat), checked on 2026-09-16, specify 1200 dpi for black-and-white line art and allow Helvetica or Arial lettering. The current JACS page does not repeat the full older structure-settings table; this preset combines that established ACS table with current JACS output guidance.

## Implementation

`engine/drawing_style.json` is the shared definition, embedded in Rust and bundled alongside the Python worker. A normal 42-unit bond in Moruno represents 14.4 publication points. Existing document coordinates stay intact. SVG declares dimensions in points; PDF explicitly converts the SVG's 96 px/in coordinate system to 72 pt/in. PNG has matching physical dimensions and 1200 dpi metadata. Screen zoom never changes exported size.

Atom labels use measured font advances and separate text runs for hydrogen subscripts, isotope masses and charges. Hydrogens choose a side based on neighboring bonds; ring double bonds use an inset second line. Annotation selection bounds and multiline spacing follow the larger default font. Arial must be available for exact font matching; fallback sans-serif metrics are used otherwise.

The preset supplies the default drawing values; saved per-object overrides and supported imported formatting remain intact. A complete document-style/stationery editor and page/column layouts remain future work. Arbitrarily dense structures still need manual inspection and cleanup. The style does not supply reaction semantics, naming, or NMR prediction.

The 2026-09-20 chain update makes **every new document** reset drawing and typography controls to JACS/ACS, including after another document used custom fonts, colors, bond lengths or constraints. The contextual JACS/ACS button also restores bond length, chain angle and constraints without modifying existing objects. Per-object overrides and imported formatting remain supported. See [chain controls and measured desktop results](chain-tools.md).

## Verification

Regression tests check physical SVG/PNG scale, line-art metadata, label clearance including isotopes, continuous ring outlines, CDXML dimensions, and chemical identity after exchange. A six-structure diagnostic drawing covers aspirin, caffeine, a chiral amine, isotopic charge labels, multiple bonds and terminal hydrogens. Its exported PDF is rendered for visual inspection; desktop checks verify the same style in the app.

The original desktop PDF matched the headless export byte for byte and embedded a subset of Arial. The desktop PNG likewise matched, measured 4087 × 4618 pixels with approximately 1200 dpi metadata, and contained only neutral RGB values. Both renderings were visually inspected. That initial style update passed 20 Rust tests and 10 Python tests, along with formatting and Clippy checks. The standalone bundle passed its engine check outside the checkout and its signature verification.

The 2026-09-20 chain update passed 83 Rust and 23 Python tests. The packaged app was checked after deliberately changing its font, size, color and bond length, then creating a new document. Exported CDXML declares Arial, 10 pt label/caption sizes, 14.4 pt bonds and 0.6 pt lines; the saved chain coordinates were also measured independently. See [the desktop verification record](chain-tools.md#reference-and-verification).

Headless export is available for repeatable checks:

```sh
cargo run --locked --example export_drawing -- input.moruno output-prefix
```

The arrow update also resets arrow tool settings on New. A desktop check changed the text style to 20 pt, bold/red and the arrow tool to a custom unequal equilibrium, then verified New restored Arial 10 pt, black and the Forward arrow with a 0.6 pt line. See [arrow verification](arrows.md).

The symbol/orbital update keeps JACS defaults on New, including the 0.6 pt graphic stroke and Arial 10 pt atom labels. Atom-owned marks default to 75% of the label size. New also resets phase/style settings and enables atom attachment. This reset is covered by the application regression test.

The symbol/orbital standalone desktop check changed text to 18 pt bold red, graphic lines to 3 pt and orbital phase reversal, then verified New restored Arial 10 pt black, JACS dimensions and default drawing tools. Evidence: `artifacts/symbols-qa-20260920/jacs-default.png`. That update passed 102 Rust and 30 Python tests, including a externally saved attached-charge export.

The ring/catalog update retains these defaults. Desktop drawings containing fused/rotated chairs, cyclopentadiene, an oxygen-connected chair and L-phenylalanine have measured 42-unit bonds (14.4 pt); their CDXML exports declare the same bond length and 0.6 pt line width. Ring PNG and rendered PDF were visually inspected. New continues to show Arial 10 pt and JACS/ACS. Current validation is 111 Rust and 32 Python tests; see [ring verification](ring-presets.md).
