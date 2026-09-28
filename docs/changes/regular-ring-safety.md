# Safe regular-ring placement

Regular rings now reject attachment sites that cannot accept their new bonds,
including saturated carbon, explicit hydrogens, abbreviations, and protected
stereochemistry. A new ring also rejects unshared vertices that coincide with an
existing atom. Rejection preserves the drawing, selection, and Undo/Redo history.

This addresses [#43](https://github.com/Ameyanagi/ReShiki/issues/43) and
[#44](https://github.com/Ameyanagi/ReShiki/issues/44). Ordinary atom sharing and
outward bond fusion remain available for rings of 3–8 atoms. Shared double bonds
keep their original order. Existing six-membered aromatic placement retains its
validated benzene planner.

The conservative overlap policy rejects the proposed placement; it does not
merge extra atoms. Proximity uses the existing aromatic planner's tolerance of
1/15 of the shared/generated bond length (with a 0.01 world-unit floor), independent
of canvas zoom. Explicit hydrogen counts, isotope/map labels, radicals, marks,
abbreviations, and affected stereo assignments are protected. Drawing-defined
wedge/hash stereochemistry is protected even before chemistry checking;
projection-only bonds are not treated as stereo assignments.

## Matched renderer evidence

Before:

![Before: ring attachment creates carbon valence six, and inward fusion silently creates four coincident atom pairs](../images/regular-ring-safety/before.png)

After:

![After: all three invalid ring placements are rejected and preserve their original molecular graphs](../images/regular-ring-safety/after.png)

The three panels use the same input, action, framing, scale, style, and renderer.
Counts and carbon valences are measured from the actual result graph and placed
outside the application-rendered drawing. The overlaid-ring defect is mostly
invisible in the figure; its four coincident atom pairs are the important graph
measurement.

| Input and action                                                                                                            | Base result                                               | Fixed result                                             |
| --------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- | -------------------------------------------------------- |
| [Four-bond carbon](fixtures/regular-ring-safety/saturated-input.rsk): click the center with the regular six-ring tool       | 10 atoms / 10 bonds; central C valence 6                  | Rejected; original 5 atoms / 4 bonds; C valence 4        |
| [Explicit CH4](fixtures/regular-ring-safety/explicit-h-input.rsk): click the carbon with the same tool                      | 6 atoms / 6 bonds; total C valence 6 including explicit H | Rejected; original 1 atom / 0 bonds; total C valence 4   |
| [Cyclohexane](fixtures/regular-ring-safety/overlay-input.rsk): start at the first bond midpoint and drag to the ring center | 10 atoms / 11 bonds; 4 coincident pairs                   | Rejected; original 6 atoms / 6 bonds; 0 coincident pairs |

The defective saved graphs are retained as
[saturated-before.rsk](fixtures/regular-ring-safety/saturated-before.rsk),
[explicit-h-before.rsk](fixtures/regular-ring-safety/explicit-h-before.rsk), and
[overlay-before.rsk](fixtures/regular-ring-safety/overlay-before.rsk).
Fixed outputs equal the input documents byte-for-byte after serialization.

Capture provenance:

- Base: `0ae0fb6a21c5e5c5b90ae66730458fda4ea2a20d` (`origin/main`).
- Head: the implementation on `fix/regular-ring-safety` accompanying this document.
  Renderer operation source (`src/editing.rs`) Git blob:
  `43163e31337efd1222188ecbc45a5c5bc0471680`.
- Platform/build: Apple Silicon macOS, Rust dev profile, native application SVG
  renderer rasterized with the same resvg dependency; captured 2026-09-29.
- Scale/framing: each 320 × 245 panel uses `viewBox="-110 -100 260 210"`;
  complete PNG is 960 × 350. Default publication drawing style, white background.
- Operation: `editing::ring_oriented`, ring size 6, `aromatic=false`, hit radius
  5 world units; atom cases click `(0, 0)` and overlap drags the first bond
  midpoint to `(0, 0)`.
- Generator: [regular_ring_safety_qa.rs](../../examples/regular_ring_safety_qa.rs),
  run unchanged against base and head. It also saves input/result `.rsk` files
  and prints graph counts, total first-carbon valence, and coincident pairs
  below 0.01 world units.

Reproduce with `cargo run --example regular_ring_safety_qa -- OUTPUT_DIRECTORY`.
Use the environment/helper configuration required by the developer guide. The
example is new in this PR; copy it unchanged into a base worktree for comparison.

## Native desktop interaction review

The same input fixtures were also exercised in the real macOS application with
Computer Use. These additional screenshots show rejection status and unchanged
whole-drawing counts in the Properties panel:

![Native application rejects attachment to saturated carbon, retains five atoms and four bonds, and still reports all changes saved](../images/regular-ring-safety/desktop-saturated-rejected.png)

![Native application rejects dragging a regular ring over itself and retains six atoms and six bonds](../images/regular-ring-safety/desktop-overlap-rejected.png)

| Desktop action                                                                   | Observed result                                                                             |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Open saturated input, select regular six-ring, click the central carbon          | Rejected; 5 atoms / 4 bonds; All changes saved                                              |
| Add a separate valid ring, Undo, click the saturated center, then Redo           | Rejection keeps Redo available; Redo restores the separate ring (11 atoms / 10 bonds total) |
| Open overlay input, drag the first bond midpoint toward the existing ring center | Rejected; 6 atoms / 6 bonds; All changes saved                                              |
| Drag the same edge outward, then Undo                                            | Valid fused ring appears; Undo restores the original cyclohexane                            |
| Open explicit-H input and click the CH4 carbon with the regular six-ring tool    | Rejected; 1 atom / 0 bonds; All changes saved                                               |

Desktop capture provenance: combined integration commit
`446331ec8ffdef3c852cccec8b13e2105d9e6737`, macOS 26.5.1 ARM64 debug build,
reported window setting 1280 × 820, Fit at 250%, optional object toolbar and
Properties panel visible. The unmodified PNG captures are 2560 × 1704 including
the native frame. This build combines the concurrent feature PRs, including this
PR's implementation `25c8159834b6eda691d2ccc1c39904dc2becf69c`; it is not a
standalone build of this PR. The matched base/head renderer comparison above is
separate. All nine before/rejection/valid-control desktop captures were inspected;
only the two useful rejection views are retained here to avoid duplicate images.
No native mid-drag snapshot was captured: preview/commit agreement is covered by
the automated canvas regression below.

## Validation

- Eight graph regressions cover atom and bond endpoint capacity, protected
  state/explicit H, drawing-defined stereo versus projection, reaction membership,
  exact and nearby duplicate vertices, every supported ring size, shared double
  bonds, valid outward fusion, native save/reopen, and invalid geometry.
- All fourteen existing aromatic-fusion regressions pass.
- App/input regressions cover rejected placement with selection/history/Redo
  preserved, valid placement as one Undo step, and matching drag preview/commit
  at zoom 0.5, 1, and 2.5.
- Real desktop interaction checks pass for all three rejection cases, successful
  outward fusion, Undo, and preservation of Redo, with the combined-source
  provenance and preview-capture limitation recorded above.

Release caption: **Regular rings reject saturated/protected attachment sites and
coincident duplicate vertices without changing your drawing.** Reuse the after
image above with its alt text.
