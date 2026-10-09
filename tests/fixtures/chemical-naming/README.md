# Historical chemical naming desktop saves

**Superseded HTTP prototype only.** The current local rule-based implementation
has a separate [original desktop package](local/README.md) and
[local review](../../../docs/chemical-naming-local-visual-review.md).

These original native files were saved through ReShiki's desktop **Save As**
command on macOS arm64 on 2026-10-09. Both contain ethanol: three atoms, two
ordinary single bonds and no annotations, arrows, graphics or groups.

- `ethanol-desktop-final.rsk`: final replay of source
  `8529b3fc17699d5b4003300e7fe4b463bf121f8a`, signed executable SHA256
  `467aaeb6b71a18a51946b831bdc5546f389f2e53f1469665a78356f667af7ce5`.
  OPSIN forward resolution, scrolled insertion, full-graph Undo/Redo and a
  separately consented PubChem lookup all passed. The exact candidate's CLI
  analysis returns valid `CCO`, formula `C2H6O` and InChIKey
  `LFQSCWFLJHTTHZ-UHFFFAOYSA-N`, without warnings.
- `ethanol-desktop.rsk`: earlier review of source
  `f4487a57f1134cc805764910666fe2687ca1f249`. Caption insertion and one Undo
  were checked before this save; no caption remains in the saved fixture.

See [the desktop review](../../../docs/chemical-naming-visual-review.md) for
captures, exact settings, service provenance and review chronology. These are
controlled chemical test data, not imported third-party drawings.
