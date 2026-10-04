# Release and descriptor tooling validation

[PR #132](https://github.com/Ameyanagi/ReShiki/pull/132), by @Ameyanagi, merged after passing CI and code review. The comparison baseline is
`81ca82101061ecc201545a3cae8d8257f03254d3`; the refactored scripts are in
`526d378fa6081716f317f4c3cd7a090e5fb3482e`.

Three internal changes share the six native release targets, reuse the existing
signed-archive checksum writer and move the descriptor operation table out of
per-node compilation. Production scripts are 10 lines smaller. Executable
validators remain separate, with their existing errors and source/provenance
guards.

## Verification

- **38 focused Python tests passed**, covering release metadata/downloads,
  native helper builds, descriptor queries, native packaging and license notices.
  Regressions check exact checksum bytes, verification before checksum writing,
  original unsigned-archive provenance, all six targets and their order, direct
  script and importlib loading, distinct malformed-header errors, and recursive
  descriptor pattern ordering.
- Changed Python files passed Ruff lint and formatting; all four changed
  production scripts passed Ty. The pinned environment used Python 3.12.12,
  RDKit 2026.03.6, Ruff 0.16.8 and Ty 0.0.82.
- Full descriptor regeneration, using the pinned RDKit source checks, produced
  byte-identical output and stdout on the baseline and refactored implementations:
  110 Crippen rules and two counters compiled into 119 queries. The generated
  JSON is 48,120 bytes, with SHA-256
  `29bf130cc684dad52888b7d3428f214e58388117d3498263e22d0130de01f397`.
  Its content equals the committed descriptor data; that file is pretty-printed,
  while both generators emit compact JSON. The committed asset was unchanged.

Signing and package subprocesses in the focused tests were mocked. This
validation did not sign, notarize, launch or publish a release package. The
refactor has no visible drawing or interface change, so its evidence is the
exact-output comparison and automated regressions.
