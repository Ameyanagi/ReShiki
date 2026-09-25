# Visual capture provenance

Captured on macOS on 25 September 2026. Drawing boards contain actual ReShiki
SVG output inside labeled HTML panels, rasterized with Chromium at 1200 CSS
pixels wide and device scale 1. The drawings were not retouched. Before/after
panels use the union of both SVG viewBoxes, giving them the same coordinate
scale and framing. The independent-resize sheet uses one shared viewBox per
row. Unrelated feature examples are individually fitted for legibility.

Desktop images are captures of actual optimized macOS applications. Their
captions identify interaction evidence separately from renderer-only evidence.
The version in these development builds remains 0.7.1; that number does not
mean these unreleased changes shipped in the public 0.7.1 release.

## Bug comparisons

The small native fixtures below retain the actual captured state on each side.
For renderer-only differences, render the same fixture at the two listed
commits. For graph-edit differences, replay the described operation on the
same starting graph. Property numbers come from the native engine's `analyze`
response, not from image interpretation.

| Comparison               | Before → after               | Reproduce                                                                                                                                                                | Captured native states                                                                 |
| ------------------------ | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| Bold double junction     | `c9b134b` → `e91bea9`        | Render the same four-carbon chain with a bold double middle bond and automatic second-line placement.                                                                    | [Before](fixtures/pr23-bold-before.rsk), [after](fixtures/pr23-bold-after.rsk)         |
| Triple geometry          | `c9b134b` → `e91bea9`        | Start with a four-carbon zigzag of length 42; change the middle bond to triple. The base uses the triple-bond preset; the head uses the new bond shortcut edit.          | [Before](fixtures/pr23-triple-before.rsk), [after](fixtures/pr23-triple-after.rsk)     |
| Ring attachment          | `6da4783` → `e91bea9`        | Open the [starting chain](fixtures/pr23-angle-input.rsk), hover its last carbon and press `3`.                                                                           | [Before](fixtures/pr23-angle-before.rsk), [after](fixtures/pr23-angle-after.rsk)       |
| Symmetric branches       | `6da4783` → `e91bea9`        | Start with a three-carbon chain at (-36.373, 21), (0, 0), (36.373, 21); press `9` at the middle atom.                                                                    | [Before](fixtures/pr23-branch-before.rsk), [after](fixtures/pr23-branch-after.rsk)     |
| Sulfonyl growth          | `6da4783` → `e91bea9`        | Use the same three-carbon chain; press `k` at the middle atom.                                                                                                           | [Before](fixtures/pr23-sulfonyl-before.rsk), [after](fixtures/pr23-sulfonyl-after.rsk) |
| Explicit NH₃ and formula | `004eaee` → `8d30e56`        | Place Pt at the origin, N at (-70, 0) and (0, -70), and Cl at (70, 0) and (0, 70), each singly bonded to Pt. Enter `NH3` at both N positions in Auto mode; run analysis. | [Before](fixtures/pr25-nh3-before.rsk), [after](fixtures/pr25-nh3-after.rsk)           |
| Whitespace reverse label | `004eaee` → `8d30e56`        | Use Boc on an N–C bond of length 70, set the reverse label to three spaces, force right alignment and rotate the selection 180°.                                         | [Before](fixtures/pr25-group-before.rsk), [after](fixtures/pr25-group-after.rsk)       |
| MgBr bond clipping       | `22085e5` → `54b23fe`        | Render the same expanded MgBr group fixture.                                                                                                                             | [Before](fixtures/pr28-mgbr-before.rsk), [after](fixtures/pr28-mgbr-after.rsk)         |
| Azide bond clipping      | `22085e5` → `54b23fe`        | Render the same expanded azide group fixture.                                                                                                                            | [Before](fixtures/pr28-azide-before.rsk), [after](fixtures/pr28-azide-after.rsk)       |
| Inner-circle clearance   | `54b23fe` → `7800561`        | Render the same arene–Fe contact in front of the ring.                                                                                                                   | [Before](fixtures/pr29-circle-before.rsk), [after](fixtures/pr29-circle-after.rsk)     |
| Tilted-curve clearance   | `54b23fe` → `7800561`        | Render the same contact and tilted inner ellipse.                                                                                                                        | [Before](fixtures/pr29-tilted-before.rsk), [after](fixtures/pr29-tilted-after.rsk)     |
| Finder opening           | Pre-fix QA build → `edb4203` | Ask macOS to open the same [case-00 input](fixtures/pr30-finder-input.rsk) in the corresponding application.                                                             | Real alert and editor captures; the exact pre-fix commit was not recorded.             |

The `6da4783` captures show the initial shortcut implementation inside PR #23;
those operations did not exist at its declared base. All other commit pairs
above use the corresponding PR base and head. PR #30's final `16e456f` commit
changes a macOS listener-count test only; its production application is the
captured `edb4203` implementation.

## New-feature sheets

| PR  | Source of the example drawings                                                                                                                                                                                                 |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| #22 | [`haworth_qa`](../../examples/haworth_qa.rs), [`haworth_interchange`](../../examples/haworth_interchange.rs), and the 14 supplied drawings at `c9b134b`.                                                                       |
| #23 | [`shortcut_qa`](../../examples/shortcut_qa.rs) at `e91bea9`; unique key/context examples selected from the 131-case gallery. Aliases and non-drawing commands are checked against `src/hotkeys.rs` and `src/app/shortcuts.rs`. |
| #24 | `LICENSE` and the contribution-license section of `CONTRIBUTING.md` at `004eaee`, displayed as documentation excerpts.                                                                                                         |
| #25 | [`label_qa`](../../examples/label_qa.rs) at `8d30e56`, including formula labels, rotation and alignment cases.                                                                                                                 |
| #26 | [`resize_qa`](../../examples/resize_qa.rs) at `75cac6d`; original, width 160%, height 60%, and height 150% variants.                                                                                                           |
| #27 | [`ring_fill_qa`](../../examples/ring_fill_qa.rs) at `22085e5`.                                                                                                                                                                 |
| #28 | [`template_shortcut_qa`](../../examples/template_shortcut_qa.rs) at `54b23fe`; all 12 template additions and four shortcut outputs.                                                                                            |
| #29 | [`ring_crossing_qa`](../../examples/ring_crossing_qa.rs) at `7800561`; figure comparisons plus the actual front/behind interaction.                                                                                            |

Run an example with `cargo run --example NAME -- /tmp/OUTPUT_DIRECTORY` at
the corresponding commit, following the normal developer setup. The resulting
native and figure files can be opened in the editor for review. Large export
corpora and scratch browser pages are intentionally not committed. Published
PNG boards are stored once in `docs/images/pr-reviews/` and reused by the PRs
and changelog pages.
