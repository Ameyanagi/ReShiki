# ReShiki 0.6 release validation

This record covers the 0.6 documentation refresh, native desktop walkthrough, and release checks. The initial screenshots used application commit `9302fd7`; the final fixes and `.rsk` support are in `107f543`. [PR #3](https://github.com/Ameyanagi/ReShiki/pull/3), [PR #4](https://github.com/Ameyanagi/ReShiki/pull/4), [PR #10](https://github.com/Ameyanagi/ReShiki/pull/10), and [PR #11](https://github.com/Ameyanagi/ReShiki/pull/11) were merged into `main` in dependency order. The merged tree at `26f58ae` matches the tested final branch. All five walkthrough issues are closed.

Updated on 2026-09-23.

## Checks

- The default Rust suite passes: 407 tests passed, none failed, two ignored.
- Commit checks pass: Cargo check, Clippy, formatting, and the configured web/Python checks.
- The optimized macOS application builds and its ad-hoc signature verifies.
- Documentation checks and JavaScript lint/formatting pass. The static build verifies 3,819 local links and assets.
- Browser checks pass for the homepage and ten guides at desktop and mobile widths, with light and dark desktop themes: 33 combinations, all images loaded with alternative text, no horizontal page overflow or JavaScript page errors. The build emits bundler warnings for Astro head-injection directives; generated screenshot styles are present and the pages render correctly.
- The bundled application passes native chemistry twice with Python, uv, and the checkout unavailable. The package contains no Python chemistry runtime.
- All five platform package checks passed in the [manual Release builds workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/35749463353). The run produced unsigned test artifacts; signing and publication were skipped.

## Screenshot walkthrough

The refreshed macOS images are direct window captures from the optimized 0.6 application, using a separate documentation workspace. They show real editable drawings and native controls. The seven Windows/Office images are retained from earlier verification at the maintainer's request; they are not presented as new 0.6 captures.

The installation guide now shows a blank canvas. The esterification example uses upright carbonyl groups and conventional bond angles. Its molecular graph and bond lengths are preserved.

All 39 existing macOS screenshots were replaced, and two were added for blank startup and assistant progress. The seven Windows/Office screenshots are the only unchanged images. Captures are 2560 × 1704 native window images.

The desktop walkthrough exercised bonds and separate chain tools, ring and arrow palettes, template connection/sharing/fusion, reusable templates, Move & attach, selection-scoped properties, cleanup, grouping, context menus, text and atom labels, abbreviation replacement/expansion, arrow bending, orbital phases, crossing order, picture import, publication pages, and view guides. The assistant walkthrough covered real progress and previews, background generation, review findings, and successful reviewed application in both acceptance modes.

A 166-atom, 144-bond hydrolysis drawing with 30 other objects was moved twice, restored with two Undo operations, zoomed and fitted in the optimized app. No persistent stall was observed in this functional walkthrough. This was not a frame-latency benchmark or a cross-platform performance claim.

The corrected esterification example preserves every atom property except position, all bonds and other document data. The maximum bond-length difference is below 0.0001 drawing units. Its SVG, PDF and PNG exports were generated successfully; the PNG was imported through the native picture workflow.

## Follow-up fixes and validation

The release-workflow follow-up addresses all five findings from the screenshot walkthrough:

- [#5: Filename extensions](https://github.com/Ameyanagi/ReShiki/issues/5). Save, Save as, figure/data export, template export and drawing-style export share extension completion. Explicit suffixes are preserved. If completing a suffix changes the destination to an existing file, ReShiki asks before replacing it. Native macOS Save, Save as and PDF export were verified with extensionless names, then their actual files were inspected.
- [#6: Upright carbonyls](https://github.com/Ameyanagi/ReShiki/issues/6). Proposals and visual corrections support 30-degree increments. Tests cover aldehydes, ketones, esters, stereochemistry and branching, including graph and bond-length preservation. A live branching request produced carbonyl angles of −90° and arrow angles of 0° and −90° (within 0.0001°). Unbalanced transformation warnings remained visible for manual review.
- [#7: Conversation scrolling](https://github.com/Ameyanagi/ReShiki/issues/7). Composer menus retain the chat widget and its scroll state. Reopening restores the previous position; **Jump to result** returns to the completed preview. Native checks covered model, effort and edit-mode menus, scrolling into history, closing/reopening, and jumping back to the result. Generation also completed while another inspector was open.
- [#8: Caption baselines](https://github.com/Ameyanagi/ReShiki/issues/8). Compound captions share a measured baseline across each horizontal reaction row. Branches retain independent participant groups. Regression tests cover different structure heights, multiline captions and coefficients. All four compound captions in the live esterification result have exactly the same vertical position.
- [#9: Inspector scrolling](https://github.com/Ameyanagi/ReShiki/issues/9). Changing inspectors resets the destination scroll position, including transitions from tool-specific handlers. Normal updates within a panel retain the position. The native check opened Reaction roles from a scrolled Properties panel and confirmed that the arrow selector and first reactant were visible.

GPT-6 Sol (`gpt-6-sol`) is now the assistant default when available in the connected account catalog; explicit saved choices are retained. The installed Codex catalog advertised text and image input for this model. Live generation and mandatory image review succeeded for esterification and branching. The esterification draft completed in 35 seconds, passed review, applied through Accept all edits, and was removed and restored with one Undo/Redo pair.

Follow-up validation: 407 default Rust tests passed, none failed, two existing manual GPU snapshot tests ignored. The optimized macOS application was rebuilt, its ad-hoc signature verified, and the affected workflows checked through native computer use. Eight manual images were refreshed again: assistant progress, review, edit modes, model, effort, accepted scheme, reaction roles, and Export. The seven Windows/Office images remain unchanged.

New documents now default to `.rsk`, using the same readable JSON format. Native macOS checks saved an extensionless name as `.rsk`, reopened a 14-atom reaction, and confirmed that the complete document matched its `.reshiki` original. Saving an existing legacy file retained its filename; Save as also accepted an explicitly typed `.reshiki` suffix. Regression coverage opens `.rsk`, uppercase `.RSK`, `.reshiki`, and `.moruno` documents. Windows installer associations include all three extensions. The three downloadable examples now use `.rsk`; their old URLs remain available with identical contents. Eight native packaging tests and the documentation checks/build passed.

The final template adjustment enables **Keep placing** by default. Regression tests build three fused rings through successive bond placements, retain the chosen anchor and connection mode, reject invalid clicks without changing the drawing, undo/redo each placement, and stop on Escape. One-off insertion remains available through the checkbox. Native release-build verification placed three cyclohexane rings without reselecting the template, used Undo/Redo and Escape, and saved 14 atoms and 16 bonds as `.rsk`. The guide includes a new direct window capture of this workflow.

## Release gates

The first signed release rehearsal was stopped before publication to include the final template-placement change. A new rehearsal will verify the final merged application. It checks all five platform packages, Windows installation and upgrades, and native chemistry with Python and uv unavailable. The macOS app and disk image must pass Developer ID signing, notarization, stapling, and Gatekeeper assessment.

Publishing `v0.6.0` repeats the package and signing checks and requires the full live chemistry reference suite on macOS ARM64, Linux x64, and Windows x64. The release workflow publishes only after those gates pass, with eight downloads and their SHA-256 checksums. A successful manual rehearsal does not publish a release.
