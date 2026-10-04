# Built-in templates follow the active journal size

After changing an ACS drawing to Nature, a newly placed built-in template
kept its ACS geometry. Free built-in insertion now uses the active drawing
style's nominal bond length. Choosing the template before or after the style
change gives the same result. Preview and commit use the same placement route.

Personal templates retain their saved geometry and independent captions or
graphics. Toolbar ring presets already use the active size. Existing atom
connections, atom sharing, bond fusion and Move & attach retain their geometry,
scoring and error behavior. This fix does not separately resize earlier objects;
the journal command retains its established document-formatting behavior.

This is an intentional behavior fix, separate from the behavior-preserving
refactors: production source grows by **59 physical lines**. Focused tests and
optional renderer evidence add 466 lines, including their test-only wiring.

## Matched before and after

The left Benzene was inserted in ACS and changed to Nature by the existing
journal command. The right Cyclohexane was then inserted from the built-in
library in empty space. Before the fix, its bonds remain approximately 42 world
units; after the fix, they match the Nature nominal size of approximately
31.496584 world units.

**Before — a new free built-in ring retains ACS size in the Nature drawing.**

<img src="../images/template-journal-size/before.png" width="798" alt="Before: the new Cyclohexane on the right has longer bonds than the Nature-sized Benzene on the left.">

**After — the new built-in ring follows the active Nature size.**

<img src="../images/template-journal-size/after.png" width="778" alt="After: the new Cyclohexane on the right matches the Nature-sized Benzene on the left.">

These are unmodified application PNG exports, copied directly from the actual
capture. Both use 1200 dpi. The original sizes are 2394 × 561 and 2334 × 456
pixels; the displayed widths are exactly one third of each source width. This
keeps the same pixel scale instead of fitting the two figures to equal widths.
Export bounds shrink with the corrected structure, so the framing differs.
The images are renderer evidence, not native desktop screenshots.

## Reproduction and source provenance

The [ACS reference fixture](fixtures/template-journal-size-acs-reference.rsk)
is the unmodified captured starting document. Open it, choose built-in
Cyclohexane in Templates, apply Nature from the Drawing style journal dropdown,
and insert in empty space on the right. Repeat with the style change before the
template choice. The fixture and both PNGs are original capture bytes; no
geometry, image scaling, cropping or redraw was applied to the saved files.

The opt-in [application harness](../../src/app/template_style_evidence.rs) uses
actual `App::update` messages for journal changes, library selection, connection
mode, toolbar selection and canvas edits. It inserts the reference at (-160, 0),
then free templates at (160, 0) with direction (195, 60). Camera zoom is 1; exported
physical scale is independent of camera zoom. SVG and PNG use the production
`export::figure` renderer. Both template-selection timings produced identical
final PNG, SVG and native-document bytes within each revision, so only one pair
is published.

Captures ran on the same macOS 26.5.1 arm64 host using the unoptimized Cargo test
profile, the same installed fonts and empty default feature selection. The
baseline production source is
`81ca82101061ecc201545a3cae8d8257f03254d3`, with only the identical evidence
harness and test-module registration added for capture. The recorded baseline
diff SHA-256 is
`0b2a15cb0321baa9a5cb0224bfa121d588538975ba482d3832f44f231f9031a8`.

The validated candidate is the combined working tree at that same HEAD plus
recorded diff SHA-256
`2bc3fc12476961ce044f74470c6b70a75dccbbc6f7c5bb51ce1088a40ce6fb1c`.
The harness source SHA-256 is identical on both sides:
`7d78ace3170232dd8bb5672e0a5ba52ba4e1f3980ad2c4b3c0e926e41fce5cb4`.
The candidate was explicitly rebuilt after a shared-target cache reuse was
detected. Only the corrected candidate capture is used here: the baseline
and rebuilt candidate each passed the ignored evidence test, with 540 and 546
other tests filtered out, respectively. The discarded cached candidate is not
evidence for this fix.

To regenerate one revision's complete capture in its matching source tree:

```sh
RESHIKI_TEMPLATE_STYLE_EVIDENCE_DIR=/tmp/template-style-evidence \
  cargo test --locked --bin reshiki \
  app::template_style_evidence::capture_template_style_renderer_evidence \
  -- --ignored --exact --nocapture --test-threads=1
```

The harness writes seven cases at three phases each, as PNG, SVG and native
JSON, plus a manifest containing actual style, selected IDs, bond lengths and
PNG dimensions/DPI. Use separate output directories and explicitly rebuild each
revision when sharing a Cargo target directory.

## Actual geometry and control parity

The following ranges come from the captured native documents and manifest,
not measurements inferred from the images. Float variation reflects the
existing rotation and translation arithmetic.

| Measurement                            | Baseline                             | Corrected candidate             |
| -------------------------------------- | ------------------------------------ | ------------------------------- |
| Existing Nature Benzene, all six bonds | 31.49658203125                       | 31.49658203125                  |
| Newly inserted Cyclohexane, six bonds  | 41.99999237060547–42.000003814697266 | 31.4965763092041–31.49658203125 |
| Journal nominal Nature bond length     | 31.496583938598633                   | 31.496583938598633              |

All five controls are byte-identical across revisions for PNG, SVG and native
documents at all three capture phases: **45 unchanged artifacts**.

| Control                                        | Result                                                          |
| ---------------------------------------------- | --------------------------------------------------------------- |
| ACS free built-in insertion                    | Existing ACS size and rendering are unchanged.                  |
| Personal ACS template inserted into Nature     | Saved ring geometry and caption are unchanged.                  |
| Connect with a bond at an existing Nature atom | Connection geometry and native result are unchanged.            |
| Fuse along a Nature bond                       | Fusion geometry and native result are unchanged.                |
| Chair A toolbar ring in Nature                 | Its existing Nature-sized geometry and rendering are unchanged. |

Each case verifies undo and redo through the App dispatcher. Connection and
fusion additionally assert their distinct atom/bond-count changes, preventing
an accidental free-placement fallback from passing as attachment evidence.
The four focused template-style integration tests passed, covering all five
journal styles, authored chair/Haworth projections, attachments with exact
serialized results, personal artwork, toolbar size and invalid-input atomicity.
The application regression compares the committed document with the pure
`Template::place` result and checks undo/redo. It does not execute the canvas
hover-preview branch. The existing template history and template-drag
cancellation regressions also passed in the recorded combined test runs. These
are local focused results, not a full-suite or cross-platform claim.

Published-byte SHA-256 values:

- Before PNG: `2f53265aa35a7ea5783a5cae89f29624f95345e87be6cf6cac2e13fc879a07d1`
- After PNG: `ce97ad660fc63eb9a15b72109e48be2192d292e3052b0e9e71d22cc24b2f4cc7`
- ACS reference fixture: `978539961782461de235eaddf593e6d1ae7cd8d3f0840d45426350ad4f466216`

## Native desktop verification

Candidate native checks completed on macOS 26.5.1 arm64 at **100% zoom**
through CUA in the isolated application
`dev.reshiki.editor.template-qa-20261004`. The actual application binary SHA-256
is `e9d67bed48074567c395777241bfb454353498802e263a7e0b2dc09ba07deb02`,
with source patch SHA-256
`2bc3fc12476961ce044f74470c6b70a75dccbbc6f7c5bb51ce1088a40ce6fb1c`.
The local build provenance is recorded in `template-desktop-build.json`; the QA
application used an isolated data directory.

The first check selected Cyclohexane while the document was in ACS, applied
Nature through the bottom journal dropdown, and committed the template in
empty space with the pointer. Escape ended the tool. Native UI Undo visibly
removed the inserted ring and Redo restored it. The actual document was saved
through the native dialog as `template-before-style-native.rsk`.
The second check started a new ACS document, applied Nature through the dropdown
before choosing Cyclohexane in the library, committed in empty space with the
pointer, and saved `template-after-style-native.rsk` through the dialog.

The saved documents contain 12 atoms/12 bonds and 6 atoms/6 bonds, respectively.
Every native bond length is in the range
31.496579999999994–31.496581374963544 world units. In the first document, the
existing reference ring and inserted ring have identical six-bond length
sequences. These values are calculated from the actual saved coordinates and
recorded in `template-desktop-results.json`. The saved-document SHA-256 values
are `f970b605bec33628f41c4d0811496961adda8e903ec0ba72bf2802b6fd1a7a21`
and `ca30d1ee0121d2019f5ee2a4bd0f17d4303e2f7071bd08d68d46bf09512b01ad`,
respectively.

The native checks establish candidate menu/library ordering, pointer insertion
and the first case's UI undo/redo. Native screenshots were observed during CUA
but were not saved or published; the unmodified renderer pair above remains
the published visual evidence. No standalone hover-movement API was available,
so isolated transient-hover routing or appearance was **not independently
verified**. Source inspection confirms that preview and commit call the same
placement function, but the current app regression tests only that function and
commit; a focused test of the canvas hover-preview routing remains a coverage
gap. Native baseline interaction and cross-platform desktop checks are not
claimed.

Reusable release caption: Built-in templates placed in empty space now follow
the current journal drawing size while personal templates keep their saved size.
