# Issue #376 evidence: typed MeO, NC, morpholino and 2-thienyl

Before: `origin/main` at 036f0c82. After: branch `fix/typed-group-spellings`.

Each label is typed on the right-hand terminal carbon of ethane (CH3–C, bond
drawn left to right) in **Automatic** mode, through the same
`atom_text::apply` that the label editor's Apply calls. `description` and
`suggestion` are the texts the editor shows under the field.

- `harness-before.rs`, `harness-after.rs`: the harness, run as a
  `reshiki-model` example against each tree. The only difference is the
  suggestion line, because origin/main has no suggestion function.
- `before/harness.txt`, `after/harness.txt`: editor description, editor
  suggestion and the Apply result (group label and reverse, atom text, or
  error).
- `before/analysis.txt`, `after/analysis.txt`: formula, SMILES and mass of
  each saved drawing (`reshiki --cli analyze`).
- `before/*.png`, `after/*.png`: `reshiki --cli render` of each drawing.
- `analyze.sh`: the analysis and rendering script.

| Typed      | Before                                              | After                                                              |
| ---------- | --------------------------------------------------- | ------------------------------------------------------------------ |
| morpholino | text on a dummy atom, CH3\*, `*C`                   | text on a dummy atom, CH3\*, `*C`; editor: "Did you mean Morph?"   |
| 2-thienyl  | Apply error "A link node requires a finite number range" | text on a dummy atom, CH3\*, `*C`; editor: "Did you mean 2-Thienyl?" |
| MeO        | text on a dummy atom, CH3\*, `*C`                   | OMe group (label OMe, reverse MeO), C2H6O, `COC`                   |
| NC         | text on a dummy atom, CH3\*, `*C`                   | text on this right-hand end (R–NC reads as an isocyanide); CN group on a left-hand end, see `after/sided.txt` |

The bond leaves the group to the left, so the after drawings show OMe and CN.
On a carbon whose bond leaves to the right, the same groups show MeO and NC.

## Follow-up: NC and OCN are read from the bond (commit 4f678044)

Read from the bond, NC on a right-hand end (R–NC) is an isocyanide and OCN a
cyanate, and the catalog has neither. So they make CN and NCO only on a
left-hand end, where they read toward the bond. `harness-sided.rs` types NC,
OCN and MeO on a left-hand end, a right-hand end and a vertical bond; its
output is `after/sided.txt`. The first after run's NC renders (a CN group on
the right-hand end) are kept in `superseded/` for reference.
