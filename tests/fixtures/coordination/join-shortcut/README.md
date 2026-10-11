# Selected donor–metal Join evidence

Original controlled Co(en)3 data and raw macOS application evidence for the
Ctrl+J / ⌘J follow-up to [PR #283](https://github.com/Ameyanagi/ReShiki/pull/283),
under review. Contribution: @Ameyanagi, project creator and maintainer. Original
code, fixtures, captures and documentation are offered under MIT OR Apache-2.0;
no external dataset or third-party implementation is added. Existing notices
remain in the repository NOTICE.

The [public guide](../../../../docs/changes/coordination-join-shortcut.md) explains
the producer applications, controls, native saves, source proof and limitations.
`manifest.json` records 49 byte-identical copies, including six untouched JPEGs.
Absolute original paths in receipts are provenance; the verifier uses repository
paths and needs only Python's standard library.

From the repository root:

```sh
python3 tests/fixtures/coordination/join-shortcut/verify.py
python3 tests/fixtures/coordination/join-shortcut/verify.py --source-root .
```

The second command requires the tested source snapshot: 1,067 broader source
entries and 532 default-app dependency inputs. Subsequent source/test changes
must be compared with that snapshot rather than weakening the hashes. Documentation
changes alone do not affect those lists. The default command verifies original
bytes, graph preservation, native history AX controls and eight semantic negative
controls. It does not launch an app, reproduce native interaction or infer arrow
visibility from graph counts.

Actual shortcut/save/reopen interaction and the platform mapping regression
passed on macOS with ⌘J. Windows/Linux Ctrl+J mapping source is present; its
non-macOS test branch and desktop exercise remain pending for this follow-up.

`co-en3-cmd-j-native.rsk` is the actual macOS Save As output, not a scripted expected
result. Its six contacts were inserted in order 5/2/13/10/9/6 → Co1. Reopening was
performed in a fresh native process. The older all-plain desktop assembly has the
same atom records and contact set, with a different insertion order; both raw
reopened views show five arrows. The upper-right N2→Co arrow's absence is an
existing drawing visibility limitation, documented separately from graph correctness.

Original receipts retain stage-specific status text. The bundle/source and local
validation receipts predate root's completed native check; `native-verification.json`
records that later check. The original before receipt's pending-after note is likewise
historical. Failed renderer diagnostics and the stopped lock-waiting Cargo check
remain recorded alongside the final passing results.
