# Mechanism arrow attachments (#92)

`before.rsk` is the frozen, actual methanol input used for native desktop review
in `ReShiki Curves Verified.app` at base `477b97f4da391c628d4e7fe5a7998f5b335b9ff3`.
SHA-256: `366b61c7f9b8bcfee1f2d1c3cfd6b22ee397ee5149b295c442d0037086977a61`.
It contains CH4O (two atoms, one bond), one positioned O lone pair, and no arrows.
At 250% with keyboard drawing off, clicking between the dots, then the carbon,
created two independent free arrows in the baseline app. The native screenshot
and separately saved resulting drawing are retained in the visual review record.
The fixture is unchanged; the version-21 links and mark ID are assigned only when
the second target click succeeds.

`two-free-arrows-before.rsk` is the separate actual baseline desktop save after
those two clicks. Its two implicit quadratic arrows remain free and retain their
exact path geometry when reopened in version 21; it is a retained baseline
control, not a candidate attachment drawing.

`desktop/` contains all 27 untouched native files from the actual desktop
review, including creation Undo/Redo, tangent edits, target/source motion,
deletion, detach actions, Escape, explicit free mode, fresh reopen and the
separate carbonyl atom/bond contexts. Their byte hashes and field checks are
recorded in `docs/reviews/mechanism-attachments/desktop-independent-check.json`.
The package manifest maps every archived file to its original coordination path.
The original native versions and serialization are retained; these files were
not normalized or rewritten for publication.

`app-two-click-test.rsk` is the distinct native result produced by the compiled
App flow test. It is the input to the signed-app CLI export/property checks,
not a claimed desktop Save As result. The actual desktop creation save is
`desktop/methanol-attached-arrow-desktop.rsk`.

See `docs/changes/mechanism-attachments.md` for the matched original screenshots,
reproduction, source/build receipts, adapter differences and review status.
The user's personal visual acceptance remains pending.
