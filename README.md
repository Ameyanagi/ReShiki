# Evidence for PR #319

Before/after captures for [PR #319](https://github.com/Ameyanagi/ReShiki/pull/319), the post-feature quality pass. They are kept out of `main`, as `docs/README.md` asks.

- `predict/`: the NMR inspector tab renamed to Predict (q-43, OD-14). Before is main 6d989010; after is the PR head.
- `labels/`: the formula look for text labels (q-64), the label editor's X₂ / X² / Plain row (q-65), and bonds from a text label (q-66).
- `panels/`: the generic repeating unit (q-46), Coordinate in place (q-45), Chemical names (q-06), reaction atom maps (q-47) and Help (q-48).
- `generic/`: PET with a tight bracket, n = 3 (q-69). Expansion was refused before the fix.

UI captures come from `ui_layout_snapshots` at 1280 × 820 light (tab bar at 1040). The label and PET images were rendered with `reshiki --cli convert` and `scene::svg`.
