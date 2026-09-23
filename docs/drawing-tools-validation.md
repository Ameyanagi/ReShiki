# Image assistant and drawing tools validation

Checked on macOS arm64, 2026-09-23, using an optimized app bundle with an isolated settings directory.

## Final review regressions

- Figure preparation now accepts validated drawing centroids without treating their contacts as a molecular graph. The regression covers SVG, PNG and PDF, hidden centroid markers and rejection of malformed target IDs.
- A CDXML round trip reproduced an unintended stereocenter from a projection-only bold bond. Editable CDXML/CDX export now rejects front-bond emphasis with instructions to disable it or use native/figure output. Plain projected bonds remain supported; native storage and figure exports retain emphasis. This restriction prevents a display setting from changing chemical identity.
- Update restart waits for unfinished atom-label edits, assistant text, attached images and image reads, in addition to unsaved drawings and generated drafts. Retried installer handoff requires a fresh acknowledgement from a running helper.

## Shared alignment toolbar and full review

The top Left/Center/Right controls now align selected chemical group labels and
captions, including mixed selections in one undoable edit. Group-only selections
show Automatic/Above beside those buttons instead of paragraph Justify. The atom
label dialog retains name, chemical/text mode and Apply/Cancel without a duplicate
alignment picker. Mixed values do not highlight a misleading alignment.

The updated app suite passed 152 tests (2 ignored). Coverage includes multiple
selected groups, untouched unselected groups, unchanged atoms/bonds, mixed
caption/group edits, paragraph defaults, inline text drafts and Undo/Redo.
All-target/all-feature Clippy passed with warnings denied. A native release-build
GUI save after Center retained every atom and bond; Automatic was restored through
the adjacent menu. The smaller dialog, ten-N batch coloring, history image after
composer removal, and enlarged image viewer were captured in the
[illustrated changelog](changes-pr17.md). The image-history check stopped generation
without applying a proposal. The changelog now indexes all 36 feature updates and
fixes audited against the PR diff.

## Atom text, sent-image history and timeouts

- Select an atom and press Enter, use Edit atom label in Properties/the context menu, or click an atom with the Text tool. M/L/X become named dummy atoms with existing connections; recognized groups such as Boc retain an expandable molecular fragment. Automatic mode prioritizes element symbols; Chemical abbreviation explicitly selects a conflicting nickname such as Ac. Text label explicitly keeps literal text on a dummy atom.
- Invalid/blank text, incompatible abbreviation attachment, tracked centroids and stale documents return errors without partial mutation. Apply is one undo step; Escape cancels. Text-tool clicks on empty space still create captions. Tests cover save/reload, copy, connections, element conversion, real Boc expansion, Undo/Redo and blocked shortcuts behind the editor.
- Each sent user message retains its own shared image. Clearing/replacing the composer attachment does not change old messages. Click the history thumbnail for a window-sized viewer with zoom/pan and Escape. Visible history stays available throughout the current conversation; only the model's textual context is limited to the most recent 24 messages. Conversation history is not yet persisted across app restarts.
- The previous unconditional 240-second generation/review limit was replaced by a five-minute inactivity limit and a twenty-minute hard limit per model turn. Relevant model events and completed canvas tools renew inactivity, while unrelated notifications cannot keep a turn alive indefinitely. Connection setup has a separate sixty-second limit; canvas tool work is cancellable and bounded. Errors identify generation versus review and the last activity; completed drafts remain available.
- Deadline tests simulate progress beyond four minutes, stalled responses and the hard cap. The [official app-server event documentation](https://learn.chatgpt.com/docs/app-server#events) was checked. Reasoning payloads are never displayed or retained for timeout tracking.
- Latest library/app tests: 174 library and 140 app tests passed, three ignored. All-target/all-feature Clippy passed with warnings denied. The new atom-text modules contain no `unsafe`, `unwrap()`, `expect()` or explicit panic; production crate guards forbid unsafe and deny panic/unchecked-indexing lints.
- Computer Use on `artifacts/atom-labels/ReShiki Labels Review.app` confirmed M/L/Boc entry, Text-tool atom editing, Undo/Redo, saving the actual Boc graph, sent-image thumbnails, enlarged-image view, Escape, blocked New while viewing, and retaining the history image after removing the composer image. Generation was deliberately stopped during this UI check; it is not a new chemistry-validation run.
- See [ChemDraw/RDKit attachment comparison](chemdraw-attachment-comparison.md) for the reproducible η³/η⁶/ferrocene and variable-attachment experiments and remaining interchange gaps.

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

## Ring curves, element colors, and coordination diagrams

- Select consecutive ring atoms, open Properties → Bond appearance, and choose **Toggle inner ring curve**. Selecting the entire ring produces a circle. The stroke follows native atom positions and retained projection depth; the original bond orders are preserved. Changing a bond preset clears its curve override. Breaking the ring restores the ordinary bond depiction.
- Choose **Atoms…** beside the color controls, pick an element and a hex color, then Apply. The default scope is the selection when one exists; **Whole drawing** applies to every matching atom. Bonds, other elements, captions and font settings are retained. Applying color and adding a curve are each one undoable edit.
- Tests cover partial and closed curves, mixed-color continuity, reversible tilt, persistence, broken rings, protection of stereochemical bonds, selected/whole-document coloring, and undo/redo.
- Source-image reconstruction can now build complete schemes from explicit coordinates, including reaction arrows, captions, atom colors, real-graph abbreviations, wildcard variable labels such as E, and inner ring curves. Instructions preserve ligand geometry around the metal and keep planar source drawings in 2D. Tilt is reserved for visible perspective or an explicit request.
- A live source-image check generated a flat 73-atom scheme with matching ligand silhouettes, Cu donor positions, blue Cu, red E, four partial ring curves, and four tBu abbreviations. The chemical check still flags a nitrogen valence assignment and the overall charges are captioned; this is a visual regression fixture, not a validated chemical structure. Native chemical errors are now included in preview/review feedback.
- Computer Use confirmed whole-drawing recoloring of ten N labels in one operation, Undo restoring their colors, and the inner-curve button creating a closed ring stroke.
- Preview and review detect internal atom/label overlaps, including crowding inside a single connected complex. Manual chemical review remains required for coordinate sketches. SVG/PDF/PNG and native files preserve the new appearance; CDXML export explicitly rejects ring curves and variable labels until lossless interchange is supported.
- ChemDraw's multi-centre and variable attachment commands were checked separately; see [the comparison](chemdraw-attachment-comparison.md). A drawing centroid and an E label do not replace those chemically meaningful node types.

## Checks

`cargo test --tests --no-fail-fast`: 441 passed, 3 ignored; two additional library regressions added afterward also pass. The ignored signed macOS staging test was also run separately and passed. The `rdkit-reference` complete aromatic response replay also passes after omitting zero projection depths from legacy serialized requests. Other reference-oracle suites were not rerun locally.

`cargo clippy --all-targets -- -D warnings`, formatting, optimized compilation, website diagnostics and the website build/link checks passed. Initial desktop checks used macOS automation. The Computer Use connection was subsequently restored and used for ChemDraw inspection and the new ReShiki controls.

## Context menu and interactive tilt follow-up (2026-09-23)

The right-click menu groups object-specific commands before clipboard actions, omits inapplicable commands, and retains its origin when moving to a smaller submenu. A dedicated left-toolbar 3D tilt tool previews screen-space drags, supports Shift snapping to 15°, and commits once on release. X/Y step buttons and front-bond emphasis remain available in its context bar.

Validation: 160 app tests passed (2 ignored), plus all 9 editing tests. New coverage checks contextual visibility, all four menu tilt directions, preserved selection and unselected molecules, Undo/Redo, zoom-independent gestures, dead zones, nonfinite input, canceled/focus-lost/outside drags, tool changes, and aromatic ring interior hits without enabling ring fusion. All-target/all-feature Clippy and formatting passed. New code contains no unsafe blocks, panicking unwrap/expect, or explicit panic calls.

Computer Use operated the optimized macOS build on an isolated two-ring document. Right-clicking the aromatic ring interior opened its contextual menu; Drag to tilt activated the new toolbar tool. A Shift-drag produced 60.000002° of X tilt. Saved bond records and the other ring's f32 coordinates were unchanged; retained XYZ bond-length error was below 0.00001 world units. Undo returned the selected ring to its plane; automated tests also cover Redo. Native screenshots: [menu](images/pr17/context-menu.png), [tilt submenu](images/pr17/context-tilt.png), [toolbar and result](images/pr17/tilt-tool.png). This is drawing-geometry validation, not coordination-chemistry validation.
