# ChemDraw diagram semantics probes

Twelve small synthetic CDXML/CDX/CDS sets isolate visibility, typography defaults,
page metadata, stroke widths, and modern/legacy spline heads. They contain no
third-party artwork. `manifest.json` records the intended source semantics;
known importer issues are observations, not desired behavior.

`generate.py` builds XML directly and uses the independent
`reference/engine/cdx_exchange.py` encoder. A local codec adaptation encodes an
unstyled `<s>` as a CDXString with zero style runs, matching the
[SDK's explicit representation](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/DataType/CDXString.htm).
This avoids the reference encoder inventing Arial/10pt runs where the XML asks
for document defaults. No ReShiki exporter creates these source files.
Each full `.cds` companion preserves the same artwork and zero-run text, adding
packed document `LabelStyle` (`0x080a`) and `CaptionStyle` (`0x080b`) defaults.
It is drawing stationery, distinct from the empty-artwork style-only controls
in `chemdraw-import-corpus`. Probe it with `--drawing-cds`.

The generator also uses native ChemDraw arrow object code **0x8021**, replacing
the published SDK/reference codec's **0x8027** alias locally. An actual native
ChemDraw spot-check displayed the CDXML arrow but omitted the CDX arrow encoded
with 0x8027. The native 0x8021 code is also documented in
`crates/io/src/exchange/mod.rs`; ReShiki accepts both codes. This correction
changes the source CDX encoding without changing any case IDs or semantics.

- Cases 01–05: [`Visible=no`](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Visible.htm)
  applies to the individual graphic, text, arrow, or curve. Hidden children in
  case 05 remain inside an ordinary integral group. Hidden artwork must stay
  absent from visible figure output.
- Cases 06–08: document caption defaults and
  [`LabelFace=3`](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/LabelStyleFace.htm)
  mean bold italic. Case 07 omits a redundant oxygen text child; case 08 adds an
  unstyled `OH` text child. Their visible oxygen typography should agree.
- Case 09: the first graphic inherits the document's 2pt
  [`LineWidth`](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/LineWidth.htm);
  the second explicitly uses 0.6pt. Both colored rectangles remain visible.
- Case 10: page bounds and
  [`PrintMargins`](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/PrintMargins.htm)
  are valid source metadata. Their loss is an existing documented limitation:
  `docs/publication-pages.md` states that molecular interchange does not retain
  publication-page metadata.
- Cases 11–12: equivalent two-segment geometry uses modern head properties or
  [legacy `CurveType=8`](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/properties/Curve_Type.htm)
  (a full arrow at the end). Both retain the two-segment geometry; case 11 has
  its distinct head/tail and case 12 keeps its legacy full head.

`Visible`, `color`, and `LineWidth` on the **group itself** were excluded. The
[published Group property list](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/Group.htm)
contains only ID, bounds, and Integral; the color/line-width applicability lists
also omit Group. These cases therefore do not rely on unverified group style
inheritance. `docs/selection-and-groups.md` also excludes arbitrary inherited
external formatting from its tested boundary.

Run the production integration controls with:

```sh
cargo test -p reshiki-io --test chemdraw_import_semantics
```

The manifest marks previously observed failures with `historical_failure`;
these observations are not rejection expectations. The original CDXML/CDX
source bytes are retained, and source-framing tests verify each added CDS
companion's inventory, text, and packed defaults independently of the app.
