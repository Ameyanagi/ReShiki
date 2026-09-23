# Image assistant and drawing tools validation

Checked on macOS arm64, 2026-09-23, using an optimized app bundle with an isolated settings directory.

## Bond rendering and connectivity

- The reported eight-atom, ten-bond drawing is retained in `tests/fixtures/bond-join-regression.rsk`. Its shared atoms are connected in the graph; the white gaps were rendering defects.
- Normal, bold and tapered outlines share bounded corners. Solid outlines and junction patches use one fill per color, preventing antialiasing seams at three-way and four-way junctions.
- Pixel tests cover the reported junctions at three raster scales in black and rust. Another 144 cases cover 3–8 member rings, three tilts, two configured widths, mixed styles and reversed bond directions.
- Solid and hollow wedge tips retain the configured normal width. Hashed wedge bars never taper below that width.
- The broader check found a dropped closing edge when clipping a hollow outline at a crossing. Its regression test checks both retained endpoint edges and the intentional crossing gap.
- Desktop inspection found flat caps on canvas line primitives while exports used round caps. Canvas caps now match exports, including ordinary/aromatic bonds and hash bars.
- Existing snapping, ring attachment, fragment joining, crossing, label, copy/paste and undo tests pass. Unconnected crossing bonds remain separate atoms.

## Assistant and projection controls

- Native Cmd+V attaches the ferrocene source beside the compact prompt, enables Send, and leaves the canvas unchanged. It does not start generation. Review/replacement controls remain accessible in the Review menu.
- A fresh connection selects GPT-6 Astra. A live image reconstruction and visual review produced two tilted five-member rings, two circles, Fe, and two tracked centroid contacts. It requires manual chemical review.
- Desktop checks confirmed Shift+R converts a selected five-member ring without changing its member count, four X +15° steps tilt its aromatic circle to an ellipse, and Add centroid creates a selectable anchor. Saved native data retains depth and centroid membership.
- Unit tests cover inverse tilt, native persistence, centroid movement/copy/deletion, invalid data, source-image review, stale-result protection and undo. Depth emphasis does not assign chemical stereochemistry.
- Centroid diagrams are drawing representations; molecular export is blocked rather than treating their contacts as validated multicentre chemistry.

## Updating

- Manual update checks work in the desktop dialog. The installed version matches the current published release, so Update and restart is disabled in this check.
- The signed macOS v0.6.1 disk image passed checksum, Developer ID, bundle identity, notarization and version verification, and was staged without replacing an installed app.
- Helper checks cover successful replacement and injected move failure/rollback on macOS and Linux package fixtures. App tests protect unsaved drawings and assistant drafts before handoff.
- Windows and Linux installation/restart have not been exercised on their native operating systems in this session. A real upgrade from this new updater still needs a later published version.

## Checks

`cargo test --tests --no-fail-fast`: 435 passed, 3 ignored. The ignored signed macOS staging test was also run separately and passed. Reference-oracle suites behind `rdkit-reference` were not part of this run.

`cargo clippy --all-targets -- -D warnings`, formatting, optimized compilation, website diagnostics and the website build/link checks passed. Desktop checks used macOS automation and captured window screenshots after the Computer Use connector became unavailable.
