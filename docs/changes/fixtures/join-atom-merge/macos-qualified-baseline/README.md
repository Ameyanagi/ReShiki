# Original Join route: qualified native rejection

On 2026-10-10 the native operator reproduced the original three-selected-atom
Cmd+J rejection in an already-built, signed macOS ARM64 app. This app includes
unrelated Generic Structures work and **is not a clean main build**. Its recorded
shipping Join source files are byte-identical to base
`51fa0991da2507bb00b27c1b420e807468de6423`:

- `src/app/shortcuts.rs`: `3716d597060d4f910faddf3780dcf762394f7faf8f6a995bd833e1c7c14ff0c5`
- `src/app/joining.rs`: `5e0b3684d7c5426ea9e22655cf996e3569938b4ba84e440c85d52161ab8140ce`
- `crates/model/src/joining.rs`: `a781b12878d08db608013ad4eec955ae9186d87e07d404785ee056b3ffc61996`

The producer pins 537 own shipping inputs and fresh own Cargo units; the operator
verified those inputs still matched. Signed executable SHA256 is
`ad1e436b20db6d94fa392f78f2b3270af28ab4381f1e3c971233b932ec1e83e4`.
Private producer receipt SHA256 is
`98cfa720206b85f52953c9bde827cbdf7145de24744ab1b6aede8923b78ded12`;
private native protocol SHA256 is
`1a85aef7035e9bc15f9a6533bbaf8eefa911ca9e89d78928e0d5026a16916adf`.
This qualifies the original Join dispatch, which rejects three IDs before calling
the model operation. The independent source confirmation remains recorded in the
[producer/baseline summary](../validation/producer-summary.json).

## Actual v3 actions

1. Open the exact unchanged version19
   [input fixture](../../../../../tests/fixtures/join-three-atoms-before.rsk)
   as a fresh document with Undo/Redo disabled. Use 250% view and keyboard drawing
   off (F8).
2. Marquee only central atoms 1/3/5 from blank canvas; the top row reports three
   selected atoms.
3. Press Cmd+J. The original diagnostic appears, the title stays clean, the
   three atoms remain selected, and Undo/Redo remain disabled. Geometry is
   unchanged.
4. Save As the separate [native rejected result](join-baseline-cmd-j-rejected-native-v3.rsk).

[Three atoms selected](../../../../images/join-atom-merge/macos-baseline-three-selected-v3.jpg)
and [Cmd+J rejected](../../../../images/join-atom-merge/macos-baseline-cmd-j-rejected-v3.jpg)
are exact v3 JPEGs, not earlier private attempts. The visible footer reads the
prefix "Select two atoms, or the four endpoints of two bonds,"; the complete
wording is independently confirmed in exact base source. Accessibility records
support the unchanged clean title and disabled history controls; screenshots
alone do not prove every action in order.

## Saved graph and comparison limits

The 3,164-byte rejected RSK has SHA256
`56a7830d476e34315af70a4c0a2956d00c446092e6bffbfdd03883cc8e6901dd`.
All six IDs, elements and coordinates match the input, with the same three plain
single edges `1–2`, `3–4`, `5–6`. N1 remains red Arial 10 pt. Native save normalizes
color/default/computed-hydrogen serialization and emits **version20** because of
that qualified app's unrelated schema work. This exact saved file is graph
evidence, not a claim that the version19 Join app reopened a version20 file.
Use the unchanged version19 input when reproducing Join.

The comparison with the
[corrected native result](../macos-native-v2/README.md) uses the same input,
Arial 10 pt labels, 250% zoom and F8-off mode. Inspector width, tab strip,
selection/cursor state and canvas framing differ between the qualified baseline
and corrected app. Original screenshots retain unrelated controlled Generic
test tabs, ordinary chrome and Recovery draft / Update available notices. The
input's closely spaced NH2/OH ink overlaps. All JPEG bytes are unretouched.

Only the valid v3 run is included. Earlier invalid attempts remain private.
There is no clean-main-binary, fresh-process restart, Windows/Linux runtime,
full Generic-feature or version20 round-trip claim in this Join evidence.
`evidence-inventory.json` pins the three unchanged originals and this summary.
