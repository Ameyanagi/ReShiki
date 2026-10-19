# Issue #377 evidence: labels on the glucose template's vertical OH bonds

Three builds were rendered, each from a clean checkout:

| Prefix  | Source                                                          |
| ------- | --------------------------------------------------------------- |
| `main`  | `origin/main` (036f0c82)                                        |
| `pr382` | `origin/fix/typed-group-spellings`, PR #382 alone (35152b2f)    |
| `after` | `fix/replaced-label-keeps-side` (stacked on #382)               |

## What each shows

- **C4 + OBn** (`c4-OBn-comparison.png`): main and #382 draw `OBn` with Bn running right over the bold front bond. After: `BnO`, the O stays at the bond and Bn reads left, the side of the template's `HO`.
- **C1 + MeO** (`c1-MeO-comparison.png`): main keeps `MeO` as a text label, and `Me` overprints the C2 vertex and its OH bond. #382 already makes the OMe group with Auto alignment, which reads `OMe` to the right, away from the ring. After: the same as #382, unchanged by this branch.

## How they were made

1. `harness/ev377.rs` was copied into `examples/` of each checkout (never committed) and run with
   `cargo run --no-default-features --example ev377 -- <ev377>/raw <prefix>`, each build with its own `CARGO_TARGET_DIR`.
   It builds the `:glucose` template document (`haworth::sugar_document(Glucose, Alpha, DEFAULT.bond_length_world)`, the same call the template library uses), types the text through `atom_text::apply` in Automatic mode (the model call behind the label dialog's Apply), and exports with `reshiki::export::drawing(doc, "png")`.
   Apply clears computed hydrogen counts until the app's chemistry check runs, so the harness restores the template's counts on the untouched hydroxyls. Without that, every other OH would render as a bare O.
2. `harness/compose.py` (`uv run --with pillow python -I compose.py raw .`) crops the label areas (`*-crop.png`) and lays them side by side (`*-comparison.png`).

The harness prints what each build made:

```
main  c4-OBn: group OBn drawn OBn (Auto)
main  c1-MeO: atom * variable Some("MeO")
pr382 c4-OBn: group OBn drawn OBn (Auto)
pr382 c1-MeO: group OMe drawn OMe (Auto)
after c4-OBn: group OBn drawn BnO (Right)
after c1-MeO: group OMe drawn OMe (Auto)
```

`raw/` holds the full renders, including the unmodified template for each build. No build `target` directories are in this folder.
