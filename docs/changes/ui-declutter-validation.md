# UI cleanup acceptance (#78)

The interface cleanup is implemented. [Issue #78](https://github.com/Ameyanagi/ReShiki/issues/78)
remains open until its combined desktop acceptance is complete. This record
separates existing evidence from the remaining native checks; preparing a
fixture or a checklist does not mark a desktop workflow as passed.

The source audit used `da5751d76db84c809b3562a3523536b459041cb8` on 2026-10-02.
[PR #98](https://github.com/Ameyanagi/ReShiki/pull/98) records a macOS walkthrough
and explicitly leaves Windows outstanding. The
[0.10.0 release validation](../release-0.10.0-validation.md) also limits desktop
interaction coverage to macOS. The [matched images](ui-declutter.md) are real
application-widget renderer output at two sizes, from the disclosed historical
commits. They are not native desktop captures of the final tabbed interface.

## Execution record

Targeted native macOS checks on `fb1bb6d`, and the `771a186` unit-input focus
retest, are recorded in the [October 2 evidence summary](issue-work-2026-10-02.md).
The same summary includes the final `ca4c52e` Linux headless renderer's Copy as
and drawing-style examples, plus the final macOS whole/selected reaction
**Copy as → system clipboard → SciFinder search** check. Both native menus
showed all 11 formats, and their copied payloads matched. These targeted checks
do not complete this native final-candidate matrix. Populate it with the exact
integrated candidate and linked results after execution. A check that was
unavailable stays **not run**; Windows CLI tests cannot replace its native
walkthrough or Office activation checks.

| Run                 | Required coverage                                                           | Current result                                                  |
| ------------------- | --------------------------------------------------------------------------- | --------------------------------------------------------------- |
| macOS               | Two-size combined interface, keyboard/focus, supported ChemDraw repeat      | Final Copy as/SciFinder check passed; complete sequence not run |
| Windows             | Two-size combined interface, keyboard/focus, EMF and Word/PowerPoint/Excel  | Not run on the final candidate                                  |
| Nightly performance | Help opening, gallery opening and loaded interactions, separately           | Not measured on the final candidate                             |
| Linux               | Compatible UI/export smoke, including the Fedora font report when available | Final headless renderer checked; native desktop/Fedora not run  |

For each run retain the commit, application version/Nightly run ID, executable
SHA-256, build mode, OS/version/architecture, renderer, display scaling and
refresh rate, logical content size, screenshot pixel dimensions, theme, zoom,
font environment, fixture hash and external-application versions. Record
whether the desktop is physical or virtual. Use an isolated
`RESHIKI_DATA_DIR` and copies of the QA drawings. After a relevant integrated
change, repeat the affected checks against the new executable.

For each result record the criterion ID, actions, expected/actual result,
pass/fail/not-run status, artifact paths, limitations, operator and reviewer.
Keep native screenshots, renderer output, returned files and performance
samples distinguishable. Do not edit a screenshot to hide a layout defect.

## Original acceptance

The IDs below preserve the issue's seven work items, six release-review items
and four additional checks. **Existing evidence** is not a claim of final
cross-platform acceptance.

| ID  | Requirement                                                                                       | Existing evidence                                                                                 | Still required for acceptance                                                 |
| --- | ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| W1  | Review default, molecule, mixed selection, arc and precise transforms together                    | Combined renderer states and PR #98 macOS walkthrough                                             | Final macOS and Windows review of all five contexts                           |
| W2  | Common actions direct; advanced/contextual controls disclosed without shrinking usability away    | Compact arrange group, Transform/More, arc strip and popovers                                     | Native discovery and hit-target review; fix only observed failures            |
| W3  | Consistent grouping, spacing, labels, icons, tooltips, units and disabled explanations            | Shared command names/requirements, shortcut labels and unit fields                                | Native platform labels/fonts and accessible control naming                    |
| W4  | Discoverable, remembered optional toolbar visibility; no extra persistent row                     | View → Arrange controls and preference round-trip test                                            | Toggle, restart and rediscover in each native application                     |
| W5  | 1280 × 820 and 1040 × 680 fit, useful canvas, stable pointer/double-click                         | Matched renderer images, row-fit and selection-stability tests                                    | Native two-size inspection and context transitions                            |
| W6  | Keyboard access, focus, names, Enter/Apply, Escape/Cancel and text protection                     | Real-widget input regressions and existing event handlers                                         | Complete changed-control keyboard/accessibility walkthrough                   |
| W7  | Preserve previews, history, chemistry, clipboard and CDX/CDXML                                    | Functional regressions and historical exchange captures                                           | Final affected tests and named post-cleanup native exchange repeats           |
| R1  | Matched before/after combined-app images at both sizes                                            | Historical renderer pairs for all requested states                                                | Final native surface captures and accurate comparison provenance              |
| R2  | Native macOS **and Windows** draw/select/reaction/transform/arc/abbreviation/ring/export sequence | PR #98 records macOS; Windows explicitly outstanding                                              | Entire sequence below on both native platforms                                |
| R3  | Editable ChemDraw abbreviation/arc clipboard and CDX/CDXML saves, repeated after cleanup          | Original checked baseline: six abbreviation examples and seven arcs on macOS ChemDraw 26.0.0.6599 | Fresh supported-workflow repeat with actual returned files                    |
| R4  | Discoverable common/advanced actions; no unexpected panel expansion or pointer movement           | Disclosure, at-most-one auto-open section and stability regressions                               | Final native transition/discovery review, including mixed-selection scrolling |
| R5  | Record and resolve defects; review the combined result before sign-off                            | Cleanup merged; missing Windows review remains documented                                         | One combined defect log, affected retests and reviewer disposition            |
| R6  | Update screenshots/help after agreed final layout                                                 | Feature docs, Help and tab/header renderer captures                                               | Update affected final surfaces and link accepted native evidence              |
| D1  | Arrow Line width (pt) separate from bond style, including mixed selection and history             | Arrow-only regression; new shared mixed-selection fixture/regression                              | Run regression and native field discovery/edit/Undo/Redo                      |
| D2  | Discoverable Copy versus Copy Image with platform guidance                                        | Copy feedback, Export command and clipboard/Office guides                                         | Check the final menu/help/feedback wording and each operation                 |
| D3  | Windows Word/PowerPoint/Excel editable Copy/paste/double-click/save workflow after cleanup        | Historical Office tests and guide                                                                 | Fresh final-candidate workflow in all three desktop applications              |
| D4  | Measure Help open, examples open and loaded interaction separately on latest Nightly              | Loaded-canvas CPU benchmarks, help redraw and gallery functional tests                            | Native per-workflow timings with build/OS/drawing size and raw samples        |

The original completed R3 checkbox remains valid historical evidence. It does
not complete the same row's explicit request to repeat the workflows after
the UI changes. Existing caption/arc format limits remain disclosed; the
cleanup does not promise universal ChemDraw compatibility.

## Native walkthrough

Run at **1280 × 820** and **1040 × 680 logical content size**, using the same
document, zoom and theme for matched states. Record physical screenshot size
and display scale separately. At minimum capture default canvas, molecule,
mixed selection/Arrange, Transform (including More), and selected arc. Keep
the historical before/after renderer pairs. If capturing native comparisons,
use the disclosed pre-cleanup combined build with an equivalent compatible
fixture; do not relabel renderer images as native screenshots or open a newer
document format in an older build. Record any unavailable before capture for
review rather than manufacturing a match.

1. Open an empty Select canvas. Inspect header/tab overflow, style/context
   rows and the open/closed inspector at both sizes. Open Arrange menus,
   inspect disabled reasons, hide **View → Arrange controls**, restart with
   the same QA profile, and verify the preference survives.
2. Draw/select a molecule. Single-click an atom, then double-click without
   moving the pointer; the atom and canvas must stay put. Move through none,
   molecule, mixed molecule/arrow/rectangle and arc selections. Check vertical
   inspector scrolling and intended section disclosure; no horizontal
   scrolling or clipped controls should be required.
3. With only a molecule selected and space to its right, press
   **Cmd/Ctrl+Shift+Right**. Confirm a reaction arrow and selected product
   copy, Undo/Redo as one action, then edit/deselect the product and use
   **Space** to recall it. Check shortcut discovery in Help.
4. Open **Properties → Transform**. Type Rotate `15` and Scale `125` without
   applying; typing must leave the drawing unchanged. Enter applies one
   field and releases text focus. Reset, edit both, then **Apply** once and
   Undo/Redo. Inspect W/H lock and units, unlocked height and **More: tilt**,
   including an edited tilt hidden by closing More. Save before/after files.
5. Draw a **120° Arc**, set Start `32` and Sweep `234.5` with Enter, choose
   **Edit arc endpoints**, drag an endpoint and Undo/Redo. Open a 360° arc at
   its coincident handle. Save/reopen and verify native arc parameters remain.
6. On a terminal carbon in `CCC`, press Enter and enter **OTBDPS** in
   Automatic label mode. Reopen the label, expand, Undo/Redo and contract.
   Repeat **TBDPS**; confirm O versus Si attachment and expected reverse label.
7. Use the [ring-rejection fixture](../../tests/fixtures/ui-declutter/ring-rejection.rsk).
   Place a valid ring in empty space, Undo, then attempt attachment to the
   methane carbon. A brief reason/red preview belongs near the pointer, with
   no modal or persistent panel. Redo must still restore the valid ring.
8. Use the [mixed-arrow fixture](../../tests/fixtures/ui-declutter/mixed-arrow-width.rsk)
   and its [exact invariant checks](../../tests/fixtures/ui-declutter/README.md).
   Find **Arrow properties → Line width (pt)** by scrolling if needed. Enter
   `1.5`, Undo/Redo and save results, for arrow-only and mixed selection.
   Bond style, geometry and unrelated objects must remain unchanged.
9. Find and use **Copy**, **Copy Image**, the Vector/Raster export choices,
   and CDX/CDXML save/reopen. Inspect PNG/SVG/PDF output. On Windows also
   export EMF and distinguish that vector picture from the editable Office
   Copy workflow below.
10. Open Import/Insert and cancel the file picker; close an unsaved disposable
    tab and Cancel the native save prompt. Check the canvas does not move
    under an added banner. Open Help, then shortcut examples; confirm an
    unchanged examples tab is reused and Save requests a personal path.
    Measure opening latency separately from this mixed functional run.

On macOS use Cmd and Option labels. Record system-intercepted key combinations
separately; PR #98 notes Escape during a copy drag reaches the app after Cmd
is released. On Windows use Ctrl/Alt and inspect actual platform fonts and
scaling. Linux may repeat compatible steps and the Fedora PNG comparison;
Windows EMF/OLE is not applicable there. Linux clipboard extensions and the
font-export issue keep their own scopes.

## Keyboard and accessibility

With the pointer parked, traverse the changed header icons, Arrange/overflow,
Transform fields/lock/More/Apply, arc controls, Import menu, Export choices,
Help and examples entry using Tab/Shift+Tab and documented shortcuts. Record
reachability, visible focus, activation and focus return after closing an
overlay. Inspect meaningful name, role and enabled state with the platform's
accessibility tools; a hover tooltip alone does not prove an accessible name.

Check Enter/Space activation where applicable and Escape/Cancel without an
unintended document edit. With numeric, plain/color text fields and inline
captions focused, type ordinary drawing shortcut letters and test modified
shortcuts. Text must retain its input; command letters must not leak into a
field. Verify the documented Enter/Undo behavior, native save-dialog focus,
and canvas edit cancellation. Record actual control/key failures for a small
targeted fix; do not mark unsupported inspection as passed.

## Post-cleanup exchange

### ChemDraw

Use the existing [six-abbreviation source](../../tests/fixtures/chemdraw-captions/source.rsk)
and [seven-arc source](../../tests/fixtures/adjustable-arcs.rsk) separately.
Record candidate and ChemDraw versions; the previously validated baseline was
macOS ChemDraw 26.0.0.6599.

1. Save the native source, then normal Copy from ReShiki and paste through the
   actual OS clipboard into ChemDraw. Record any style/Change Settings choice.
2. Verify that groups and paths are editable, Copy back into a fresh ReShiki
   tab and save the actual native return.
3. In ChemDraw use GUI Save As for CDX and CDXML. Open both in ReShiki and save
   their native returns separately. Retain originals, hashes and provenance.
4. Compare independent chemical identity, Si/O attachments, hidden members,
   group/reverse labels, caption text/styles and cubic geometry. Follow the
   [existing identity expectations](../../tests/fixtures/chemdraw-captions/README.md)
   and documented format tolerances. Do not conceal displacement with separate
   image crops or generate expected ChemDraw returns with ReShiki.

Externally returned arcs may be ordinary editable cubic paths rather than
native parametric arcs. Existing caption origin/line-height quantization and
unsupported ChemDraw-native angular arcs remain explicit limits. Historical
evidence from another build/version is not a fresh repeat.

### Windows Office

For **each** desktop Word, PowerPoint and Excel, record application version
and use a local disposable document:

1. Select the source in ReShiki and **Ctrl+C**, then paste. Inspect
   **Paste Special → ReShiki drawing object** and record physical extent.
2. Double-click, edit a visible label/bond in ReShiki and **Ctrl+S** while the
   Office document stays open. Verify Office accepts the update.
3. Close the editing window, save/reopen the Office document, double-click
   again, and verify editable native objects. Preserve the Office file and a
   copied-back native drawing when available.
4. Repeat **Copy Image** separately to demonstrate a static figure. Record
   Excel's host fill/outline separately from ReShiki's transparent preview.

Use the exact registered candidate; the [Windows guide](../windows.md#edit-a-drawing-in-office)
explains installed/portable registration. Mac Office and Linux do not provide
this Windows OLE workflow. If #107 changes descriptor bytes in the candidate,
repeat Office checks after integration and share that evidence with its owner.

## Help and gallery timing

There is no numerical #78 performance threshold established by the existing
records. First measure the three requested workflows; any proposed budget
must be labelled as a proposal. Open a focused performance follow-up only for
a reproducible measured bottleneck, with the responsible path identified.

Use the latest available Nightly and record its run ID, commit and executable
hash. If the final integration candidate differs, retain a separate series.
Measure on an idle optimized build with fixed renderer, viewport, zoom and
profile; do not run builds, profiling or another benchmark concurrently.

| Workload           | Start                                                      | Finish / separate cases                                                                                                                                                                                |
| ------------------ | ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Help opening       | F1 or Help activation                                      | First complete visible dialog; test Escape responsiveness. Empty canvas and loaded gallery separately; first open and warm reopen separately.                                                          |
| Examples opening   | Activate **Open shortcut examples** with Help already open | First complete, usable gallery frame. First open without an examples tab and unchanged-tab reuse are distinct cases.                                                                                   |
| Loaded interaction | An actual input event after gallery has settled            | First visible response for selection, short drag/drop, zoom and O/C edit/Undo. Record chemistry-label completion separately from immediate response, and drag frame gaps separately from opening time. |

The bundled gallery has 483 atoms, 429 bonds, 132 captions, 16 abbreviations
and one arrow; record the actual counts used. A four-gallery stress sample is
optional and does not establish a new acceptance requirement.

Suggested sampling is five fresh-process first opens and twenty warm opens,
then thirty discrete interactions and three ten-second drags. Keep each raw
sample, report sample count/median/p95/maximum and state the uncertainty of
small cold samples. Fresh process does not mean the OS file cache was flushed.
Use native event/presentation tracing or a time-coded recording with visible
input markers. State frame-rate quantization; automation-tool wall time is not
application latency. Do not add independent CPU medians to claim end-to-end
latency. The [existing benchmark dataset](../performance/data/README.md)
explains its CPU-only limits and how to investigate a measured slow path.

## Automated checks and completion

The new mixed-arrow test and the existing ring-rejection test load the same
fixtures used in the walkthrough. This preparation does not record a Cargo
test execution. Run these against the integrated candidate and retain results:

```sh
cargo test --locked --bin reshiki arrow_width_in_mixed_selection_preserves_bonds_and_other_objects
cargo test --locked --bin reshiki regular_ring_rejection_preserves_selection_history_and_redo
cargo test --locked --bin reshiki layout_snapshots -- --ignored
cargo test --locked --bin reshiki selection_layout_and_double_click_regression -- --ignored
cargo test --locked --bin reshiki a_selection_opens_at_most_one_section
cargo test --locked --bin reshiki inspector_preferences_preserve_drawing_selection_and_history
cargo test --locked --test adjustable_arcs --test tbdps_abbreviations
```

Run affected native clipboard/exchange, transform/arc/history tests and the
required repository checks after relevant fixes. #96 owns pivot correction;
#103 owns font/export correction; #89 owns physical-unit parsing/display;
#107 owns native OLE/printing changes. Share final evidence and repeat affected
surfaces after their included changes. #62 owns package/antivirus provenance:
a blocked Windows artifact must be recorded, not bypassed. Use isolated
profiles so QA does not contend with #56 template-lock tests. New LibreOffice
or Copy-as-format work is separate from this acceptance record.

Close #78 only after all original rows have linked, current evidence and a
combined reviewer disposition. Resolve demonstrated layout/interaction
failures, retain any measured performance follow-up allowed by the issue,
and update affected screenshots/help. An unavailable required native check
keeps the issue open; publication of a release does not waive that check.
