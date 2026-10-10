# Compact NMR palette validation

Native desktop review of the compact revision is complete on the exact preserved executable below. [PR #272](https://github.com/Ameyanagi/ReShiki/pull/272) remains under review. The [matched before/after captures](nmr-visual-review.md#matched-native-before-and-after) document the revision after user review rejected the previous full-width results dock.

## Reviewed source and preserved bundle

- Production source: `b85119828444eec36189134c93e8d1d4918fe343`.
- Platform/profile: macOS arm64, Rust 1.99.0, default-feature debug build.
- Local bundle: `.coordination/apps/ReShiki NMR Compact.app`, kept separately from the original `ReShiki NMR.app`.
- Signed executable SHA-256: `9d29f842dfb72748ba931332a7ce37e43183a6155a07b3a260118df9a5c42321`.
- Bundle manifest SHA-256: `9b36e9e863fcb949bbdbc09b52ca6d4692cb1584922ee6d1b1897321e2db5b41`.
- Source manifest SHA-256: `4fdeb8e09524b947e9249e18c6ca6884ab38bddf5333e43b7f7f03c2f01ad8c7` for 2,573 tracked files at the production commit.

All 1,072 own-worktree Rust/Cargo entrypoints were refreshed before compilation, with content hashes unchanged by the refresh. The production source hashes remained unchanged during the native build. The final dependency file references this worktree and no other feature worktree. Repository bundle packaging included the license inventory; macOS arm64 binary verification and `codesign --verify --deep --strict` passed. The original dock bundle retains its executable digest `7e891f6bfbb835fb33376af71c0c2c0c2c452eaefc5ffd0dda5be88c45eae8ca`.

Local provenance is retained in `.coordination/nmr-compact-build-provenance.json`, `nmr-compact-source-refresh.json`, `nmr-compact-source-manifest.json`, `nmr-compact-bundle-manifest.json` and `nmr-compact-depfile.d`. These paths are development artifacts, not public release downloads.

## Checks performed

| Check                                                                     | Result    |
| ------------------------------------------------------------------------- | --------- |
| App NMR tests, including all opt-in renderer checks                       | 5 passed  |
| Chemistry NMR tests                                                       | 6 passed  |
| IO NMR data, identity and TSV export tests                                | 11 passed |
| Existing tool-flyout pointer/keyboard ownership renderer regression       | 1 passed  |
| App all-target/all-feature Clippy with warnings denied                    | Passed    |
| App all-target check without default features                             | Passed    |
| Workspace Rust formatting, NMR Markdown formatting and diff checks        | Passed    |
| Locked default-feature native app build and bundle signature verification | Passed    |

The renderer checks use the actual app view at 940×620 and 1280×820 logical sizes with Properties shown and hidden. They verify unchanged drawing viewport and camera through opening, details expansion, height adjustment, hiding/revealing results, nucleus switching, closing and reopening. They click a real ethyl-acetate row and confirm its atom selection without document edits; all four compact carbon rows fit. Empty palette areas own scrolling and their pressed gestures, including release outside, while the drawing still receives input outside the palette. A real ten-carbon report scrolls to later sites without panning the drawing. Accessibility metadata and actions remain available. The 940×620 cases are renderer/input checks: the native application minimum is 1040×680. Native review below separately exercises the preserved macOS app.

## Completed native review

The native desktop reviewer launched the exact signed compact bundle, used the unchanged [ethyl-acetate fixture](../tests/fixtures/nmr/ethyl-acetate.rsk), and completed the following native checks. The app was quit after review. Evidence packaging then independently verified capture-byte hashes, the signed executable digest, original fixture/export bytes and the final saved graph; no new Cargo build or GUI launch was used for documentation.

| Native check                                                                                | Observed result                                                                                                                                               |
| ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Same 1280×820 content window, JACS / ACS Publication, keyboard drawing off, inspector shown | Before opening, both candidates start at 250%; the old dock refits to 149%, compact opening retains the original viewport and 250% camera.                    |
| Compact nucleus switching                                                                   | Three H-group medians and all four carbon values match the existing fixture. Default 320-pixel width; H 214-pixel / C 244-pixel height.                       |
| Actual carbonyl row click                                                                   | Atom #4 selected in the drawing; Undo disabled; no document edit.                                                                                             |
| Details and height                                                                          | Reference columns expand; height 250→270→250; method, conditions, limitations and attribution scroll; drawing stays at 250%; Compact restores the small view. |
| Hide/Show and close/reopen                                                                  | Four results survive hide/show. Reopening is compact with ¹³C selected.                                                                                       |
| Palette input ownership                                                                     | Padding wheel and press/drag/release outside leave camera at 250% and Undo disabled.                                                                          |
| Outside drawing input and geometry change                                                   | All six atoms selected and translated 40 pixels; four results retained; header Undo restores geometry.                                                        |
| Chemistry invalidation and restoration                                                      | Erasing C #1 clears results and disables Copy/Export; Undo and choosing ¹³C restore all four rows.                                                            |
| Copy and native Export…                                                                     | Copy clicked, without clipboard readback. Native carbon TSV is byte-identical to the existing acceptance export.                                              |
| Native minimum, inspector shown/hidden                                                      | Actual 1040×680 logical content size, 2080×1424 physical capture with 32-pixel logical title bar; all four carbon rows visible in both states.                |
| Final saved native drawing                                                                  | Clean with Undo disabled; six atoms/five bonds, original atom IDs and positions, original bond endpoints/order/display and C4H8O2 restored.                   |

macOS clamps native resize attempts to the existing minimum in [src/main.rs](../src/main.rs). Native minimum captures display 204% after resizing; this is separate from the stable 250% palette interactions at 1280×820. The 940×620 cases remain renderer-only coverage.

The [evidence manifest](nmr-compact-native-evidence.json) records each unmodified JPEG/JFIF capture, its source, dimensions, displayed zoom and SHA-256. The compact native export equals [ethyl-acetate-carbon-desktop.tsv](../tests/fixtures/nmr/ethyl-acetate-carbon-desktop.tsv), SHA-256 `23464aba142911f3fb3c635069f4bc8f2a8c0028934c6925d3065c036058b876`. It includes all four carbon rows and original provenance/limitations. The original fixture, export and historical images remain unchanged.

The [native final drawing](evidence/nmr/ethyl-acetate-compact-desktop-final.rsk), SHA-256 `08978db95416b8f58df2d8b12cbe949328327c8ddc0d0a75c7e32668e0f14cca`, and [independent comparison](evidence/nmr/compact-desktop-independent-check.json) establish the restored graph. Version 19 adds ordinary default fields and empty figure collections; the existing codec omits the old fixture's bond ID keys while retaining atom IDs 1–6. This is graph-restoration evidence, without an NMR-result persistence claim.

The ChemDraw reference remains bounded: installed 26.0.0.6599 is Prime and lacks ChemNMR, as confirmed by [Revvity support](https://support.revvitysignals.com/hc/en-us/articles/4408233427220-ChemDraw-ChemNMR-options-do-not-appear-in-the-Structure-menu). Bundled official help describes a separate prediction window. It supports separating prediction results from the drawing layout; no live ChemNMR visual match was available or claimed. ReShiki predicts linked shifts and does not simulate a spectrum.
