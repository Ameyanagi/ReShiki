# Issue #380 evidence: salt names in Import → Name

- **Before:** `origin/main` at 036f0c82.
- **After:** branch `fix/salt-name-message` at 96d0c67b.

## How it was produced

`harness.rs` is an ignored tokio test. It was appended to `src/app/naming/import_dock/tests.rs` only for the capture and is not committed. For each name it runs the real path:

1. `Action::Name`, then **Insert** (`insert_input`).
2. The real `resolve_preview`, which uses the OPSIN worker and the app's own executable (`RESHIKI_NAMING_HELPER`).
3. `Action::Finished`.

It then prints the `naming::Error`, the `NoticeKind`, the visible summary (`Notice::summary`) and the Details text. Last, it renders `App::import_panel()` with iced's headless renderer, with Details expanded, to a 320×560 PNG.

Run with:

    EV380_OUT=<dir> RESHIKI_NAMING_HELPER=<abs>/target/debug/reshiki \
    RESHIKI_INCHI_HELPER=<abs>/artifacts/inchi-helper/reshiki-inchi-helper \
    cargo test --locked --no-default-features --bin reshiki evidence_380 -- --ignored --nocapture

## Results

| Name | Before summary | After summary |
|---|---|---|
| sodium chloride | Unsupported or ambiguous name. Try a more specific name. | Salts and mixtures aren't supported yet. Draw the ions, or paste SMILES under Import → Structure. |
| sodium acetate | Unsupported or ambiguous name. Try a more specific name. | Salts and mixtures aren't supported yet. Draw the ions, or paste SMILES under Import → Structure. |
| reshiki-no-such-chemical-name-50 | Unsupported or ambiguous name. Try a more specific name. | Unchanged |

- The Details text is the same before and after, and nothing is inserted in any case (0 atoms).
- The full text output is in `before/output.txt` and `after/output.txt`.
- The panel renderings are in `before/*.png` and `after/*.png`.
