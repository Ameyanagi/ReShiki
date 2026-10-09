# Assistant image benchmark

Baseline `51fa0991da2507bb00b27c1b420e807468de6423` / Nightly `0.11.0-nightly.20261008.37859845267.1`.

Requested `gpt-6.1-sol` / `xhigh` / default tier; `codex-cli 0.161.0`. Each run records the effective configuration.

13/16 planned workflows attempted. Finite references: 6 exact completed graphs / 6 completed / 11 attempted / 12 planned.

| Case                     | Repeat | Status        | Seconds | Graph stage | Exact identity | Review issues |
| ------------------------ | -----: | ------------- | ------: | ----------- | -------------- | ------------: |
| ethanol                  |      1 | completed     |    46.8 | completed   | yes            |             0 |
| gly-l-ala                |      1 | completed     |   164.9 | completed   | yes            |             1 |
| cyclic-gly4              |      1 | failed        |    74.1 | —           | —              |             0 |
| boc-l-ala-sodium         |      1 | completed     |   148.8 | completed   | yes            |             1 |
| gly-l-ala-low-resolution |      1 | completed     |   169.8 | completed   | yes            |             1 |
| cyclic-gly4-low-contrast |      1 | failed        |    57.9 | —           | —              |             0 |
| gly-l-ala-cropped        |      1 | completed     |   226.8 | completed   | —              |             3 |
| ethanol-unreadable       |      1 | clarification |    44.6 | completed   | —              |             0 |
| ethanol                  |      2 | completed     |    45.1 | completed   | yes            |             0 |
| gly-l-ala                |      2 | completed     |   169.5 | completed   | yes            |             1 |
| cyclic-gly4              |      2 | failed        |   232.9 | provisional | yes            |             0 |
| boc-l-ala-sodium         |      2 | failed        |    87.7 | provisional | yes            |             0 |
| gly-l-ala-low-resolution |      2 | timeout       |    28.0 | —           | —              |             0 |
| cyclic-gly4-low-contrast |      2 | not_run       |       — | —           | —              |             0 |
| gly-l-ala-cropped        |      2 | not_run       |       — | —           | —              |             0 |
| ethanol-unreadable       |      2 | not_run       |       — | —           | —              |             0 |

Full atom/bond/stereo/fragment/abbreviation errors, messages, apply validation, repeated graph variation and configuration are in `report.json` and per-run `score.json`.

- Small synthetic original cohort, not a real-scan or general chemistry accuracy estimate.
- Native candidate acceptance and model visual-review completion do not prove chemical graph identity.
- Provisional previews are scored separately and never counted as completed successes.
- No full haptic/variable/coordination reference policy; the rhodium handoff fixture is excluded.
- Strict protonation, tautomer and stereo identity; aromatic/Kekule and ordinary explicit/implicit H normalize.
- Error localization uses bounded common topology matching and is descriptive, not a unique edit-distance proof.

Not exposed by this application backend; unavailable, not estimated.
