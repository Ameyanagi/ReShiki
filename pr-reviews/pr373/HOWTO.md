# COSMolKit bump comparison harness

`bump_compare.rs` is a self-contained Cargo example for the `reshiki-io` package.
It uses only APIs that are unchanged by the COSMolKit 0.3.0 -> 0.5.0 migration:
`reshiki_io::chemistry::{smiles::read, inchi::kernel::{Molecule::prepare, generate, read}, inchi::key::from_inchi}`
(re-exported reshiki-chemistry) and `reshiki_geometry::{solve, Request, Response}`.
Never commit it.

## Run (identical in either worktree)

```sh
S=/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad
WT=$S/ck-base            # or the migration worktree
TD=$S/ct-base            # that worktree's private CARGO_TARGET_DIR
cp $S/ck-compare/harness/bump_compare.rs $WT/crates/io/examples/bump_compare.rs
cd $WT && BUMP_OUT=$S/ck-compare/after.json CARGO_TARGET_DIR=$TD \
  cargo run -q -p reshiki-io --example bump_compare
rm $WT/crates/io/examples/bump_compare.rs   # keep the worktree clean
```

- Dev profile (same as the tests; `cosmolkit-core` is opt-level 3 there). ~2 min total, ~1 min build.
- `BUMP_ONLY=inchi` or `BUMP_ONLY=geometry` runs one half.
- Fixture paths are resolved from `CARGO_MANIFEST_DIR/../../native/geometry/tests/fixtures/`.

## Output

- `inchi[]` (86 cases): SMILES(+CX) -> optional state mutation -> `kernel::Molecule::prepare` -> `kernel::generate`:
  status, InChI, InChIKey (`key::from_inchi`), message, log, AuxInfo, diagnostics, and a
  round trip (`kernel::read` -> regenerate, `same` flag). Parse/prepare errors are recorded as strings.
- `inchi_read[]` (17 cases): direct InChI strings through `kernel::read` (+ regeneration) and `key::from_inchi`, incl. invalid ones.
- `geometry[]` (21 molecules x MMFF94/MMFF94s/UFF): `generate` (4 conformers for SMILES cases,
  fixture counts otherwise; seed 0x52534b; 500 iterations), `evaluate_generated`, `relax_perturbed`
  (generated coords + deterministic 0.15 A LCG noise, 2000 iterations, heavy-atom Kabsch RMSD vs generated
  and vs perturbed input), and `evaluate_common_geometry` (the first field's generated geometry evaluated under
  every field, isolating force-field changes from embedding changes). `heavy_rmsd_vs_seed` is set when
  Generate received seed coordinates (C60 reference XYZ). `iterations` is always null: the public
  `Response` does not expose the minimizer iteration count (only `converged` and `iteration_limit`).
  `embedding_failure_counters` is parsed from the solver diagnostics (format `index:count`).

## Comparing

Ignore every `elapsed_ms` / `total_elapsed_ms`. Everything else was bit-identical across two runs at main,
except entries with `timing_sensitive: true` or the `*-8conf` fixtures: those hit ETKDG's wall-clock
embedding timeout (counter 11) and their counters/conformer selection vary run to run. Use the `*-1conf`
fixtures as the deterministic Taxol/C36 comparison points. Expected format change after the bump: the
failure-counter vector grows from 12 to 15 causes (new 12 Minimization, 13 KtermViolation, 14 Clash), so
indices >= 12 may newly appear in `embedding_failure_counters`.

Quick diff:

```sh
python3 - <<'EOF'
import json, sys
S='/private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ck-compare/'
def diff(a, b, p=''):
    if isinstance(a, dict):
        for k in sorted(set(a) | set(b or {})):
            if k.endswith('elapsed_ms'): continue
            diff(a.get(k), (b or {}).get(k), p + '/' + k)
    elif isinstance(a, list) and isinstance(b, list) and len(a) == len(b):
        for i, (x, y) in enumerate(zip(a, b)): diff(x, y, f'{p}/{i}')
    elif isinstance(a, float) and isinstance(b, float):
        if abs(a - b) > 1e-6 * max(1, abs(a)): print(p, a, b)
    elif a != b: print(p, str(a)[:120], '|', str(b)[:120])
diff(json.load(open(S + 'before.json')), json.load(open(S + 'after.json')))
EOF
```
