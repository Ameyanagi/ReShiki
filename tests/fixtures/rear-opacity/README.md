# Rear-opacity desktop fixtures

These are byte-for-byte copies of actual macOS native Save As files from ROOT's
review of source `7ec621227bc6fd777d5918efb9b8b6443c207d8e`, signed executable
`4d35fe616bc9cb1388e4a6ec45312c65cc21aa6b67e42e17b077dd848fcc4043`.
The baseline was saved by the parent Projection app as native version 19.
Candidate saves are version 22. All retain the same C60, 60 atoms, 90 bonds,
complete chemical fields and XYZ coordinates.

| Files | Purpose |
| --- | --- |
| `c60-rear-opacity-before-desktop.rsk` | Actual typed parent input; SHA-256 `121280660ab89d7a53a7f0a336d5c23edc4c7a46e8dafe143443c9ff884a7d6e`. |
| `c60-rear-opacity-100.rsk` | Default candidate: only the native version differs from the typed input. |
| `c60-rear-opacity-{25,0}.rsk` | Actual Apply results; only depth appearance is added to the 100% document. |
| `c60-rear-opacity-{25,0}-{undo,redo}.rsk` | One-step history saves; exact expected native bytes are restored. |
| `c60-rear-opacity-{25,0}-fresh-reopen.rsk` | Fresh-process persistence; exact bytes match the corresponding Apply saves. |
| `c60-rear-opacity-25-noop.rsk` | Repeat Apply25 leaves the native file and disabled history unchanged. |

The six public images are original native JPEGs in
[`docs/images/rear-opacity`](../../../docs/images/rear-opacity), with capture
conditions and title/tab differences explained in the
[review](../../../docs/rear-opacity-review.md). Private incidental-UI captures
are excluded. The [manifest](evidence-manifest.json) pins all public copies.

Run `python3 scripts/verify_rear_opacity_evidence.py` from the repository root
to check all eleven complete native documents, their frozen hashes,
history/persistence byte equality, and the retained f32 depth classification.
Native replay needs only standard-library Python 3.9+ and requires no Cargo,
GUI or chemistry service. The optional `--export-dir` flag checks the
independent signed-CLI outputs with Pillow when those separately retained
exports are available; they are excluded from this compact public package.
Saved files establish data equality; the desktop receipt establishes observed
input actions and history state. The immutable
[independent audit](validation/independent-desktop-verification.json) also pins
the six JPEGs, signed app, 1,069 compiled source inputs and 15 fresh artifacts.

The C60 fixture originates from the existing project-owned
[projected-double-bond fixtures](../projected-double-bonds/README.md), optimized
and tilted once before either capture. It is not a vendor drawing. Original
fixtures and screenshots are contributed under MIT OR Apache-2.0.
