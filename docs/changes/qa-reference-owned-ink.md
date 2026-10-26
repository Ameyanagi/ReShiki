# Rigid owned ink in the reaction reference test

The [scheduled Linux reference run](https://github.com/Ameyanagi/ReShiki/actions/runs/40295742094)
on 26 October 2026 failed the shared
`reaction_projection_includes_owned_map_and_stereo_ink` unit test in five
independent binaries. Each reported `[reaction-ink] Spacing 6.373991 != 0`.
The failure came from projecting a synthetic document and checking its final
rendered extent; the logged comparison of real native reaction drawings passed.

The synthetic fixture contains a fixed conditions caption, while its map and
stereo indicators used automatic collision placement. Translating one
participant relative to captions, separators and other atoms can change an
automatic indicator's position relative to its owner. The projection's rigid
translation assumption therefore did not hold for this fixture. The logs do
not identify the exact obstacle or resolved Linux font.

The owned-ink unit now gives its map and stereo indicators explicit offsets on
opposite outer sides of the participant. Separate visibility checks require
map ink to extend the left bound and stereo ink to extend the right bound
beyond atom-label-only extents. The original spacing and metadata assertions
remain. This changes only test input and assertions: the independent
projection, spacing tolerance, full-response comparisons, negative controls
and frozen worker are unchanged.

The separate `mapped_reaction_imports_keep_visible_ink_spacing` check uses
`LocalEngine` to import reaction SMILES for mapped methanol/water and a
mapped chiral reaction with an ammonia agent. It exports each real drawing as
V3000 RXN, then reads and finishes that RXN through the production reaction
API. Both drawings are checked directly with the existing spacing checker;
the synthetic projection is never called. The check requires visible map
`123456`, computed CIP on the chiral input, and unchanged map values through
RXN. It uses the actual import defaults: maps visible, stereo indicators
hidden, with automatic indicator positions.

On macOS 27.0.1, Apple M1, arm64, the seven shared projection tests pass,
including the unchanged negative controls. The separate production test also
passes: two RSMI imports and their two RXN round-trips satisfy the existing
strict spacing checks. Validation used base `b956e463` plus these test changes,
an offline locked debug build, and two build jobs. Run the
`reaction_presentation::tests::` filter and the exact
`mapped_reaction_imports_keep_visible_ink_spacing` test in the
`reaction_drawing` target with the `rdkit-reference` feature.

An initial charged borohydride/sodium probe passed RSMI spacing, then reached
the RXN reader's existing `query reaction atom` rejection. That restriction
remains unchanged; the supported round-trip checks use neutral inputs.
Original Linux failure logs and both charged-probe logs are preserved in the
release completion audit. Hosted Linux reference validation remains pending.

This fixture correction does not establish that every automatic-indicator
collision case in production is clear. Production reaction layout has a
related one-pass measurement dependency; a failure through a real import
would need its own regression and correction. No application drawing or UI
behavior changes here, so a before/after drawing comparison does not apply.

Release caption: **Keep the reaction reference test strict while measuring
map and stereo ink with deterministic owned positions.**
