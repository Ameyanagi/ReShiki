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

## Validation

- Eight graph regressions cover atom and bond endpoint capacity, protected
  state/explicit H, drawing-defined stereo versus projection, reaction membership,
  exact and nearby duplicate vertices, every supported ring size, shared double
  bonds, valid outward fusion, native save/reopen, and invalid geometry.
- All fourteen existing aromatic-fusion regressions pass.
- App/input regressions cover rejected placement with selection/history/Redo
  preserved, valid placement as one Undo step, and matching drag preview/commit
  at zoom 0.5, 1, and 2.5.
- Renderer evidence demonstrates graph and drawing output; it does not replace
  a real desktop interaction check. Desktop verification is pending integrated
  application review.

Release caption: **Regular rings reject saturated/protected attachment sites and
coincident duplicate vertices without changing your drawing.** Reuse the after
image above with its alt text.
