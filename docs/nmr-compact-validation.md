# Compact NMR palette validation

Native desktop review is pending. PR #272 remains draft after user review rejected the previous full-width results dock. The [earlier dock captures](nmr-visual-review.md#previous-dock-capture-and-reproduction) remain historical functional evidence.

## Preserved candidate

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

The renderer checks use the actual app view at 940×620 and 1280×820 logical sizes with Properties shown and hidden. They verify unchanged drawing viewport and camera through opening, details expansion, height adjustment, hiding/revealing results, nucleus switching, closing and reopening. They click a real ethyl-acetate row and confirm its atom selection without document edits; all four compact carbon rows fit. Empty palette areas own scrolling and their pressed gestures, including release outside, while the drawing still receives input outside the palette. A real ten-carbon report scrolls to later sites without panning the drawing. Accessibility metadata and actions remain available. These are renderer/input checks, not native desktop acceptance.

## Native review to complete

Use the unchanged [ethyl-acetate fixture](../tests/fixtures/nmr/ethyl-acetate.rsk), JACS / ACS Publication style, keyboard drawing off, and the same window bounds and initial Fit state for the old and compact candidates. Record the viewport and displayed zoom before opening prediction and after opening, expanding, resizing and closing it. Compare the old dock with the compact palette at the same native window size, then capture expanded details separately. Test the inspector shown and hidden, atom #4 selection, palette and outside drawing input, and a native TSV export retaining all four carbon rows and provenance. Keep original capture bytes and record their actual encoding. No new native acceptance or visual result is claimed in this record.

The ChemDraw reference remains bounded: installed 26.0.0.6599 is Prime and lacks ChemNMR, as confirmed by [Revvity support](https://support.revvitysignals.com/hc/en-us/articles/4408233427220-ChemDraw-ChemNMR-options-do-not-appear-in-the-Structure-menu). Bundled official help describes a separate prediction window. It supports separating prediction results from the drawing layout; no live ChemNMR visual match was available or claimed. ReShiki predicts linked shifts and does not simulate a spectrum.
