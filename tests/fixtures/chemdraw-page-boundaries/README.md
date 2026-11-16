# ChemDraw page-boundary controls

These synthetic CDXML and full-drawing CDS pairs preserve two independently
found import failures and single-page controls. No ReShiki exporter creates
the inputs. Run `PYTHONDONTWRITEBYTECODE=1 python3 tests/fixtures/chemdraw-page-boundaries/generate.py`
to reproduce them with the independent Python reference codec.

- `line-height`: three 72pt lines inherit a 216pt paragraph line height. A
  second page must follow the complete caption, keeping its line spacing.
- `wrapped-caption`: 36pt text inherits a 108pt line height and wraps in a
  144pt box without explicit newlines. Page arrangement must measure the
  effective paragraph layout.
- `large-label-*`: box-less oxygen labels use 72pt root defaults or an explicit
  atom text child, including hydrogens stacked above or below. Two page-local
  atom positions must not leave these labels overlapping.
- `xyz-origin`: the minimal accepted single-page `p="0 0" xyz="0 0 1"`
  oxygen is repeated on two pages with distinct object IDs.
- `fractional-xyz`: a nondefault24pt drawing scale and negative fractional
  fixed-point XYZ value exercise translation across zero. The 2D projection
  matches the native reader's original integer XYZ interpretation.
- `mixed-xyz`: the same native3D fragment also contains a p-only atom. Its
  original native position and the fragment's3D status must remain intact.
- `hybrid-fragments`: that mixed native3D fragment shares a page with an
  ordinary2D fragment, exercising both translation domains on the same page.

All cases require disjoint visible page content and preserved within-page
relative positions. The imported model combines pages on one canvas and
reports that page settings/boundaries are not retained.

CDS uses the documented CDX framing plus packed LabelStyle/CaptionStyle
defaults. Numeric paragraph line heights use native twentieths of a point;
the reference codec's auto/variable sentinel handling is unchanged. The first
reference-produced CDS bytes are retained in `encoder-baseline/`: its legacy
unscaled lineheight packing describes 0.15x spacing rather than the XML's 3x
spacing, and is outside the supported paragraph range. The corrected companions
encode the intended values without changing the source XML or test assertions.
Those four diagnostic files are expected rejection cases, excluded from the
manifest's accepting cases.
Unstyled text uses zero style runs as defined by the
[CDXString specification](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/DataType/CDXString.htm).
Before binary packing, XYZ values are canonicalized to the native reader's
signed16.16 truncation; this prevents the independent encoder's rounding from
changing the paired source's original integer geometry. See the
[2D/3D coordinate data types](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/DataType/CDXPoint3D.htm)
and [Page object](https://iupac.github.io/IUPAC-FAIRSpec/cdx_sdk/Page.htm).
