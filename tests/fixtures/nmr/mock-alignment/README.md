# NMR mock-alignment evidence

This packet records the accepted mock-aligned floating/docked NMR UI in PR [#272](https://github.com/Ameyanagi/ReShiki/pull/272). [Visual guide](../../../../docs/changes/nmr-floating-mock-20261010.md).

From the repository root, using only Python's standard library:

```sh
python3 tests/fixtures/nmr/mock-alignment/verify_evidence.py
```

The verifier checks the indexed original bytes, the primary native AX rows/selection/history, whole-byte input/Save-as equality, the native TSV against its existing qualified fixture, the fixed accepted source/dependency mapping and receipt cross-links. It does not rebuild the application or repeat compiler/native audits. `--app /path/to/ReShiki\ NMR\ Final\ Verified.app` additionally checks the nine recorded bundle files if that preserved local app is available.

| Record                                                                                                                                                          | Scope                                                                                                |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| [Byte/path index](evidence/index.json)                                                                                                                          | Generated mock, matched BEFORE, 15 raw AFTER JPEG+AX captures and original receipts/logs             |
| [BEFORE qualification](evidence/before-native-qualification.json)                                                                                               | V3 app/source attribution and keyboard-OFF manual 160% baseline                                      |
| [Native acceptance](evidence/final-native-acceptance.json)                                                                                                      | Only the actual final macOS controls and export/save observations                                    |
| [Independent native audit](evidence/native-acceptance-audit.json)                                                                                               | Capture/AX, camera-mask, application and byte-equality verification                                  |
| [Artifact acceptance](evidence/final-artifact-acceptance.json)                                                                                                  | Independent completed source, 42 tests, strict checks and signed build audit                         |
| [App provenance](evidence/build/package/final-app-provenance.json)                                                                                              | Frozen source, default build, 15 fresh own artifacts, 568 dependency inputs, nine-file signed bundle |
| [Frozen source](evidence/build/source-manifest.json)                                                                                                            | All 2592 pre-publication file hashes                                                                 |
| [Dependency inputs](evidence/build/build/dependency-inputs.json)                                                                                                | The 568 actual shipping inputs, with original absolute paths retained                                |
| [App results](evidence/build/focused-app/result.json) / [named log](evidence/build/focused-app/tests.log)                                                       | 29 named PASS                                                                                        |
| [Canvas results](evidence/build/focused-canvas/result.json) / [named log](evidence/build/focused-canvas/tests.log)                                              | 9 named PASS                                                                                         |
| [Marker results](evidence/build/focused-markers/result.json) / [named log](evidence/build/focused-markers/tests.log)                                            | 1 named PASS                                                                                         |
| [Cache results](evidence/build/focused-cache/result.json) / [named log](evidence/build/focused-cache/tests.log)                                                 | 3 named PASS                                                                                         |
| [Strict Clippy](evidence/build/clippy/result.json), [default check](evidence/build/default-check/result.json), [format/diff](evidence/build/format/result.json) | Completed acceptance gates on the frozen runtime inputs                                              |
| [Native TSV](evidence/final-carbon13.tsv)                                                                                                                       | Exact 1322 bytes, same as `../ethyl-acetate-carbon-desktop.tsv`                                      |
| [Input](evidence/ethyl-acetate-nmr-v3-native.rsk) / [Save as](evidence/ethyl-acetate-nmr-final-native.rsk)                                                      | Exact same 3377 bytes, including every serialized field                                              |

The receipts retain their original local paths and historical timing, including a build-time “native pending” entry before the later accepted native receipt. Those fields have not been rewritten. The generated mock is not a native screenshot. Raw JPEG, AX, JSON, logs, RSK and TSV retain their original bytes and terminal newlines under the existing `tests/fixtures/**` formatter exclusion; no ignore or attributes were broadened.

Only `docs/nmr-prediction.md` and `.github/workflows/checks.yml` differ from the frozen existing-file inventory after build. Additional guide/evidence files do not enter the 568 shipping-input map. The verifier checks those two approved final authored hashes separately and requires all other frozen bytes, including runtime/model/data/licenses, to remain exact. The Windows-only workflow has 38 static test matches (29 app + 9 canvas, including 19 opt-in); at packet capture on 2026-10-10, new-head CI and normal commit hooks were pending. See [PR #272](https://github.com/Ameyanagi/ReShiki/pull/272) for subsequent hook and CI results.

No J, intensities/integrals, multiplets or second-order simulation are included. Native clipboard readback, ordinary SVG/PDF exports and unperformed dense/minimum/stale/tab scenarios are not claimed. Project creator and maintainer: @Ameyanagi; original contributor and third-party attribution remain intact.
