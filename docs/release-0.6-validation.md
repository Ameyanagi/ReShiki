# ReShiki 0.6 release preparation

This record accompanies the 0.6 documentation refresh. The application source is commit `9302fd7`, with the toolbar changes in [PR #3](https://github.com/Ameyanagi/ReShiki/pull/3) and assistant changes in [PR #4](https://github.com/Ameyanagi/ReShiki/pull/4). Release publication has not been triggered.

Prepared on 2026-09-23.

## Checks

- The default Rust suite passes: 399 tests passed, none failed, two ignored.
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

## Findings

- [#6: Assistant upright-carbonyl correction limits](https://github.com/Ameyanagi/ReShiki/issues/6). Quarter-turn-only corrections cannot satisfy every requested orientation; the draft correctly remains for manual review.
- [#7: Assistant conversation scroll position](https://github.com/Ameyanagi/ReShiki/issues/7). Opening a composer menu can jump back to the start of the conversation.
- [#8: Caption baseline alignment](https://github.com/Ameyanagi/ReShiki/issues/8). Compound captions in one reaction row can appear at different heights.
- [#5: Save should append .reshiki when the user enters a filename without an extension](https://github.com/Ameyanagi/ReShiki/issues/5). Reproduced with the native macOS Save dialog. The manual advises keeping the extension until this is fixed.

- [#9: Inspector scroll position](https://github.com/Ameyanagi/ReShiki/issues/9). Opening Reaction roles from a scrolled Properties panel can hide its initial controls above the viewport.

## Publication sequence

Review the documented findings and merge the prepared PRs in dependency order: toolbar, assistant, then documentation. Retarget each dependent PR after its base merges and require passing checks on the resulting main commit. Run the signed macOS packaging check from `main` after merging, then publish a matching `v0.6.0` tag when ready. The unsigned documentation build is not a substitute for notarization or the release workflow's signing gates.
