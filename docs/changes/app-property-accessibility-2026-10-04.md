# Aromatic editing, properties and accessibility ownership

[PR #133](https://github.com/Ameyanagi/ReShiki/pull/133), contributed by
@Ameyanagi, is under review. Three small changes remove temporary copies while
preserving the existing editing and native accessibility behavior:

- Aromatic editing borrows the owned request drawing to determine its scope,
  then moves the original drawing out when replacing it with a fragment.
  Validation and fallback errors retain their order; unrelated drawing content,
  atomic history and late-response rejection retain their existing contracts.
- Property subscriptions construct one request key. Whole-drawing analysis and
  cached successes or failures still stop polling; fragment membership uses
  document atom order, and selection, revision and file-epoch changes reject
  stale cache entries.
- The native accessibility action registry retains only enabled semantic IDs and
  roles. Every control still participates in full snapshot validation, native
  tree publication and ID registration. Disabling and re-enabling a control
  retains its native ID; removed targets, invalid snapshots, inappropriate
  actions and stale application contexts remain rejected.

Production source is **6 lines smaller**: aromatic editing 0, property
subscriptions −1 and the accessibility registry −5. Tests add **181 lines**.
No dependencies or unsafe code were added. Active-tab routing, Wayland QA event
recording and deferred completion replay were not changed.

## Validation

The combined candidate checkout passed **807 tests**: 314 library tests and
493 application tests, with no failures and 60 ignored tests. The run used:

```sh
cargo test --locked --lib --bin reshiki -- --test-threads=1
```

The run included all five new regressions, verified by their test names in the
output. They cover aromatic request and validation-error parity, cached property
failures and key changes, disabled/re-enabled native IDs, current roles and value
limits, and invalid snapshot revocation. Existing gallery-content, undo/redo,
late-response and accessibility context tests also passed.

Formatting and diff checks passed. The final strict Clippy rerun remains a
separate required check. Public CI from clean checkouts for commit
`43ff4e8b833c2087dc09dc3aebbf927bccff6652` passed on
[macOS](https://github.com/Ameyanagi/ReShiki/actions/runs/37207865402/job/111452793740),
[Linux](https://github.com/Ameyanagi/ReShiki/actions/runs/37207865402/job/111452793777)
and [Windows](https://github.com/Ameyanagi/ReShiki/actions/runs/37207865402/job/111452793728).
The local run recorded base commit
`81ca82101061ecc201545a3cae8d8257f03254d3` and combined-candidate diff SHA-256
`8b78963f46a590f6681fb4d1afaa2856f8be83179383a4ecdee8ce212691499d`.

These automated checks provide behavior evidence for an internal ownership and
registry change with no intended visible output. No application-wide allocation,
RSS or timing improvement was measured, and this note makes no screenshot or
pixel-parity claim.
