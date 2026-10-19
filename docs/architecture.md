# Architecture

ReShiki owns its editable document and chemistry operations in Rust. The pinned Rust InChI kernel runs in an isolated worker mode of the same executable, as do 3D geometry, chemical names and several platform services. Python/RDKit is retained only as an optional development reference.

```mermaid
flowchart LR
    UI[Iced controls and canvas] --> App[Messages, history, selection]
    App <--> Doc[Rust document]
    Doc --> Scene[Vector scene]
    Scene --> Canvas[Iced geometry]
    Scene --> SVG[SVG export]
    App --> Local[ChemistryEngine / LocalEngine]
    Local --> Rust[Rust chemistry and drawing interchange]
    Rust <--> InChI[Same executable in isolated InChI worker mode]
```

## Worker modes

A worker mode relaunches the same executable for one bounded job. `main` checks the first argument against the table in `src/workers.rs` (`reshiki::workers::run`) before graphics, Office registration or Tokio start, so a deadline or memory failure ends only the worker.

| First argument                                                           | Platforms    | Started by                                                         | Job                                                    |
| ------------------------------------------------------------------------ | ------------ | ------------------------------------------------------------------ | ------------------------------------------------------ |
| `--inchi-worker`                                                         | All          | `crates/chemistry/src/inchi/generator.rs`                          | InChI generation and reconstruction                    |
| `--geometry-worker`                                                      | All          | `crates/io/src/geometry/client.rs`                                 | 3D geometry with the bounded MMFF/UFF and ETKDG solver |
| `--naming-worker`                                                        | All          | `src/naming/client.rs`                                             | One name parse or one name generation                  |
| `--emf-worker`                                                           | Windows      | `src/metafile.rs`                                                  | Plays an imported EMF file into its PNG preview        |
| `--emf-file-worker`, `--emf-office-worker`                               | Windows      | `native/windows/src/metafile_worker.rs`, for retained EMF pictures | Records an EMF figure file or an Office object         |
| `--libreoffice-preview`, `--libreoffice-clipboard`, `--libreoffice-copy` | All          | The optional LibreOffice extension (`src/libreoffice.rs`)          | Preview, clipboard read and clipboard write            |
| `--clipboard-worker`                                                     | macOS, Linux | `src/clipboard.rs`, `src/clipboard/linux.rs`                       | One clipboard read or write                            |
| `--print-worker`                                                         | macOS        | `src/printing.rs`                                                  | Print dialog and printing of a bounded PDF snapshot    |

The flags are distinct, so the order of the table cannot change which worker runs. LibreOffice owns its own flags and is checked when the table has no match. [Runtime safety](runtime-safety.md) records the limits of the naming and EMF workers.

## Crates and layering

The library is split into workspace crates. Each crate depends only on the crates before it:

1. `reshiki-chemistry` (`crates/chemistry/`): the chemistry core, without GUI dependencies.
2. `reshiki-model` (`crates/model/`): document, scene, styles, editing, pictures, storage and themes, plus the drawing-to-molecule and abbreviation adapters in `chemistry/`.
3. `reshiki-io` (`crates/io/`): chemistry engine, drawing interchange, export, recovery, document styles and the template library, plus the CDXML, MOL, reaction and cleanup adapters in `chemistry/`.
4. `reshiki-agent` (`crates/agent/`): the assistant's GUI-free proposal schema, layout, composition, sketch diagrams, review and canvas inspection and rendering. It also hosts the experimental operation layer `ops`: `ToolHost`, `HeadlessHost`, the session document store, budgets, cancellation and the neutral tool catalog, and `access`, the folder grants of the agent file tools. Its transports are `reshiki-mcp` and the root crate's `reshiki::cli`, started by `reshiki --mcp` and `reshiki --cli` (`src/launch.rs`).
5. `reshiki-mcp` (`crates/mcp/`): the experimental MCP transport behind `reshiki --mcp`: bounded stdio line framing, admission, backpressure and cancellation tracking, the rmcp protocol layer, the bridge to a `ToolHost` and the content-free stderr log. It uses no filesystem APIs.
6. `reshiki` library (`src/lib.rs`): a facade that re-exports the moved modules at their existing `reshiki::<module>` paths, plus the Codex assistant client and its preferences, clipboard, printing, updates, hotkeys, Office and accessibility services, [chemical names](#chemical-names) (`naming`), EMF import (`metafile`), the [worker modes](#worker-modes) (`workers`) and the experimental `reshiki --cli` commands.
7. `reshiki` executable (`src/main.rs`, `src/launch.rs`, `src/session.rs`, `src/app/`, `src/canvas/`, and `src/desktop/` on Windows and Linux): the Iced application, and the headless `--mcp` and `--cli` modes selected by the first argument.

Leaf crates depend on no layer crate; only `reshiki-windows` uses another leaf, `reshiki-metafile`. The crates above use them:

- `reshiki-metafile` (`crates/metafile/`): bounded EMF framing checks and the physical picture size, plus the picture size limits for every format. `reshiki-model` and `native/windows` use it.
- `opsin` (`crates/opsin/`): a native Rust port of the OPSIN 2.9.0 name parser, under OPSIN's MIT license (`crates/opsin/LICENSE`). Its internals follow upstream (`crates/opsin/PORTING.md`). ReShiki code meets it only in `src/naming/worker.rs` and `src/naming/catalog.rs`.
- `reshiki-process` (`native/process/`): platform limits for the naming worker's child process. All of its platform FFI stays in this crate.
- `reshiki-process-heap` (`crates/process-heap/`): the bounded allocator of worker processes and the adaptive [calculation resource policy](chemistry-resource-policy.md).
- `reshiki-depict-math` (`crates/depict-math/`): the paired math call for the pinned depiction ABI, used by `reshiki-chemistry`.
- `reshiki-geometry` (`native/geometry/`): the bounded MMFF/UFF and ETKDG geometry solver, used by `reshiki-io`.
- `reshiki-macos`, `reshiki-windows` and `reshiki-linux` (`native/macos/`, `native/windows/`, `native/linux/`): platform services such as the clipboard, printing, document handoff and Office integration, as each platform supports them.

`reshiki::chemistry` combines the core with both adapter layers, so existing imports keep working. Crate boundaries enforce the layering: a lower crate cannot import a higher one. The layer crates are workspace default members, so `cargo test` and the pre-commit checks cover them. The leaf crates are workspace members but not default members: `cargo test --workspace`, as in the [native checks](development.md#native-checks) and CI, also runs their tests. Release builds still produce exactly one executable with `cargo build --release --bin reshiki`; `build.rs` stays in the root package. The agent API adds no executable: `reshiki --mcp` and `reshiki --cli` are modes of the same binary, like the chemistry workers.

## Modules

| File                                                        | Responsibility                                                                 |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `crates/model/src/document.rs`                              | Atoms, bonds, text, arrows, stable object IDs, validation, undo/redo snapshots |
| `crates/model/src/scene.rs`                                 | Toolkit-independent lines, polygons and labels; `scene/svg.rs` writes SVG      |
| `crates/model/src/scene/bond_pass.rs`                       | Two-stage bond preparation: owned inputs, then a borrowed pass                 |
| `crates/model/src/scene/projected_bonds.rs`                 | Double-bond rails in their retained ring face; the per-scene face index        |
| `crates/model/src/scene/attachment_ink.rs`                  | Visible-ink hit tests for new mechanism-arrow targets                          |
| `crates/model/src/depth_appearance.rs`                      | Depth paint scopes: RGB fade, rear-opacity alpha, RGB materialization          |
| `crates/model/src/rear_opacity.rs`, `rear_opacity/`         | Rear-opacity paint, view visibility, clipping and alpha-resolved ink           |
| `crates/model/src/generic.rs`, `crates/model/src/generic/`  | Generic definitions, validation, edit guards and bounded finite expansion      |
| `crates/model/src/reaction_mapping.rs`, `reaction_mapping/` | Atom maps and the bounded Auto-map graph search                                |
| `crates/model/src/molecule_names.rs`                        | Linked name captions and the canonical identity that naming shares             |
| `crates/model/src/arrow_anchors.rs`                         | Mechanism-arrow target links and their reconciliation                          |
| `crates/model/src/graphics/path.rs`                         | Pen path nodes and their independent Bézier controls                           |
| `crates/model/src/editing/reference.rs`                     | Reference-edge alignment and single-bond stretch                               |
| `crates/model/src/joining.rs`, `crates/model/src/joining/`  | Move & attach, multi-atom merge, in-place coordination and the Join route plan |
| `src/canvas.rs`                                             | Canvas module root: canvas types, camera, event and draw dispatch              |
| `src/canvas/hit.rs`                                         | Hit testing for pointers and regions, and drawn-bond attachment targets        |
| `src/canvas/snapping.rs`                                    | Where gestures land: move deltas, smart guides, arrow ends, ring anchors       |
| `src/canvas/input/`                                         | Pointer and keyboard events: presses, motion and per-gesture releases          |
| `src/canvas/paper/`                                         | Paper drawing: background, active-gesture preview, document and overlays       |
| `src/canvas/previews.rs`                                    | Thumbnail, inspector, palette and proposal canvas programs                     |
| `src/app.rs`                                                | App state, messages, lifecycle, update entry points and the root view          |
| `src/app/gates.rs`                                          | Ordered modal gates that run before message dispatch                           |
| `src/app/dispatch.rs`                                       | Message dispatch table: one handler per message variant                        |
| `src/app/canvas_edit/`                                      | Canvas gesture dispatch, direct manipulation, placement and clicks             |
| `src/app/assistant.rs`, `src/app/assistant/`                | Assistant actions, request lifecycle, attachments, drafts and panel view       |
| `crates/agent/src/`                                         | Assistant proposals and canvas tools; the experimental `ops` operation layer   |
| `src/launch.rs`                                             | First-argument `--mcp`/`--cli` dispatch, heap ceiling, grants and shutdown     |
| `src/workers.rs`                                            | Worker-mode flags and their dispatch, before the GUI starts                    |
| `src/session.rs`                                            | Startup arguments and host edit sessions (Office, LibreOffice, Microsoft 365)  |
| `src/desktop.rs`, `src/desktop/`                            | Windows and Linux desktop handoff, host election and Office launcher proxies   |
| `src/naming.rs`, `src/naming/`                              | Canonical identity, naming rules, aliases, `--naming-worker` mode and client   |
| `src/naming/catalog.rs`, `src/naming/catalog/`              | Alias index keyed by `naming::canonical_smiles`; loaded once, fails closed     |
| `licenses/name-aliases/`                                    | Alias data terms: OPSIN MIT and ChEBI CC BY 4.0, listed in `NOTICE`            |
| `src/metafile.rs`                                           | EMF import through the `--emf-worker` playback process on Windows              |
| `src/cli.rs`, `src/cli/`                                    | Experimental `reshiki --cli` commands on an in-process `HeadlessHost`          |
| `crates/mcp/src/`                                           | Experimental MCP stdio framing, protocol and `ToolHost` bridge                 |
| `src/app/bond_edits.rs`                                     | Bond drawing settings and edits to selected bonds                              |
| `src/app/atom_edits.rs`                                     | Element, charge, isotope, radical and mark edits to selected atoms             |
| `src/app/ring_edits.rs`                                     | Ring tool settings and the selected ring's aromaticity                         |
| `src/app/selection_edits.rs`                                | Selection, grouping, framing and arrangement commands                          |
| `src/app/history.rs`                                        | Undo and redo of drawing history                                               |
| `src/app/view_settings.rs`                                  | Window, camera and view-aid preferences                                        |
| `src/app/engine_jobs.rs`                                    | Chemistry engine requests and the handling of their results                    |
| `src/app/workspace.rs`                                      | Command bar, context row, compact palette, inspector and status bar            |
| `src/app/icons.rs`, `src/app/icons/`                        | Original vector tool and command icons; one submodule per icon family          |
| `src/app/nmr.rs`, `src/app/nmr/`                            | NMR per-tab state, detached job, floating palette, docked report and plot      |
| `src/canvas/nmr.rs`                                         | Session-only NMR atom and shift labels on the canvas                           |
| `src/app/naming.rs`, `src/app/naming/`                      | Chemical names inspector, the Import dock name field and name captions         |
| `src/app/naming/aliases.rs`                                 | Alias and name controls of **Import → Name → Details** and the inspector       |
| `src/app/generic.rs`                                        | Generic-structure edits and **Expand generic structures** into a new tab       |
| `src/app/reaction_mapping.rs`                               | Reaction inspector: atom-map edits, Auto-map jobs and the review preview       |
| `src/app/numeric_transforms.rs`, `numeric_transforms/`      | Exact numeric transforms, reference align/stretch drafts and field focus       |
| `src/app/office.rs`                                         | Office save targets, owned by each document tab                                |
| `src/app/desktop.rs`                                        | The UI's answer to desktop open requests on Windows and Linux                  |
| `crates/io/src/engine.rs`                                   | Chemistry interface, native routing and response validation                    |
| `crates/chemistry/src/`                                     | Molecule preparation, bounded graph, properties and stereo calculations        |
| `crates/chemistry/src/nmr.rs`                               | HOSE-style NMR environments (`reshiki-hose-v1`) and observed-shift lookup      |
| `crates/io/src/nmr.rs`, `crates/io/src/nmr/dataset.rs`      | NMR request, fingerprint, bundled index, prediction; nmrshiftdb2 transform     |
| `crates/model/src/chemistry/`                               | Drawing-to-molecule preparation and abbreviation detection and replacement     |
| `crates/io/src/chemistry/`                                  | CDXML, MOL and reaction interchange, and selection cleanup                     |
| `crates/model/src/pictures/exchange/`                       | Bounded raster decoding, orientation, transparency and reflection              |
| `crates/model/src/editing.rs`                               | Clipboard remapping, transforms, component arrangement and ring placement      |
| `crates/io/src/recovery.rs`                                 | Atomic session snapshots and recovery candidates                               |
| `src/clipboard.rs`, `src/app/clipboard.rs`                  | Native multi-format Copy/Paste, asynchronous completion guards and safe Cut    |
| `native/macos/src/clipboard.rs`                             | Bounded single-item AppKit pasteboard bridge                                   |
| `native/windows/`                                           | Windows clipboard, printing and editable Office objects through a safe API     |
| `crates/io/src/exchange/`                                   | Editable drawing export, bounded CDX/CDXML codec and legacy text encodings     |
| `reference/engine/cdx_exchange.py`                          | Python reference codec retained for differential tests                         |
| `crates/io/src/export.rs`                                   | Vector PDF and raster PNG from the shared SVG scene                            |
| `crates/model/src/storage.rs`                               | Write complete files beside the destination, then atomically replace           |
| `crates/model/src/style.rs` and `assets/drawing_style.json` | Shared JACS / ACS defaults, publication units and font advances                |
| `reference/engine/worker.py`                                | Independent Python/RDKit reference, enabled by `rdkit-reference`               |

Document coordinates use screen-style positive-down Y, with 28 world units per RDKit coordinate unit. The default single bond is 42 world units, representing 14.4 publication points in the JACS / ACS preset. The camera never changes stored coordinates or export size. Native documents are saved as JSON format version 26 and accept versions 1–26 when reading; see [document format versions](#document-format-versions). Version 17 stores theme palette colors by name and custom colors as displayed; reading an older dark-canvas drawing converts its custom colors once to the lightness-flipped values it showed. Version 16 embeds custom themes and version 15 adds explicit reaction roles tied to arrow and atom IDs. A drawing from a newer ReShiki reports the document version it needs instead of a parse error. Version 14 adds validated per-document drawing settings; the physical coordinate scale remains fixed at 14.4/42 points per world unit. New presentation fields prompted version increments so older editors reject unsupported documents. History and camera are session state.

Atoms retain formal charge, isotope, explicit-H count, implicit-H policy, map number and tetrahedral winding. Winding refers to an explicit ordered list of stable neighbor IDs. Molecule preparation compensates for neighbor permutations, preventing array reordering from reversing a stereocenter. Double-bond stereo stores its reference atoms separately. Topology edits invalidate affected stereo and derived labels; a background refresh recomputes chemistry. Explicit Check also refreshes computed properties.

## Document format versions

`VERSION` in `crates/model/src/document.rs` is the newest format this build
reads, and every saved file is marked with it. Its doc comment holds the table
below; update both together. ReShiki 0.12.0 saves version 26. ReShiki 0.11.0
reads versions up to 19, so it cannot open a drawing saved by 0.12.0 and reports
the version it needs.

| Version | Added                                                                           |
| ------- | ------------------------------------------------------------------------------- |
| 1       | Original native drawings.                                                       |
| 2       | Arrow styles.                                                                   |
| 10      | Collapsed abbreviations.                                                        |
| 11      | Per-bond crossing depth.                                                        |
| 12      | Physical publication page layouts.                                              |
| 13      | Embedded pictures.                                                              |
| 14      | Document drawing styles.                                                        |
| 15      | Reaction membership and roles.                                                  |
| 16      | Embedded custom themes (later also palette hues).                               |
| 17      | Exact custom colors on either canvas; newer-version probing.                    |
| 18      | Persistent atom and bond highlights.                                            |
| 19      | Editable 3D depth appearance and captured projection stereo (#146; 0.11.0).     |
| 20      | Retained EMF picture sources (#296); cubic arrow controls on a parallel branch. |
| 21      | Arrow target links and attachment IDs (cubic-arrow branch).                     |
| 22      | Persisted rear opacity (parallel branch).                                       |
| 23      | Not used.                                                                       |
| 24      | EMF sources, cubic arrows, target links and rear opacity integrated (#297).     |
| 25      | Linked molecule-name captions and reaction atom-map labels (#298).              |
| 26      | Generic structures and coordination joining (#300; 0.12.0).                     |

## Chemistry requests

`ChemistryEngine` accepts typed import, analysis, cleanup, abbreviation, aromatic-display and export requests. Native operations prepare immutable snapshots and return a complete response or an error. Parse, chemistry, layout and helper errors never start a Python fallback.

The optional `rdkit-reference` feature exposes `engine::PythonEngine` from `crates/io/src/engine/reference.rs`. Its original JSONL worker protocol remains available for differential tests: request IDs, `protocol: 1`, `ok` responses and stderr diagnostics. The reference process uses a prepared interpreter; it never installs packages. Custom backends remain available through `LocalEngine::with_backend`.

Iced tasks keep chemistry work off the UI thread. A document revision prevents late analysis/import/cleanup from replacing newer edits. Export uses the snapshot requested. Save records the exact snapshot written and a document epoch, so a late save cannot mark later edits as saved or redirect another document's path.

## Rendering and interaction

Both the canvas and SVG consume the same vector primitives. Canvas text is emitted as glyph outlines to maintain drawing order. SVG retains editable text and depends on compatible fonts in the viewer. Label placement and collision handling are still approximate.

Pointer motion is read from each event, rather than only Iced's latest cursor snapshot. This matters when multiple move/press/release events arrive in a single batch. The desktop drag test found this issue; a regression test now reproduces that event sequence.

An Iced sensor reports actual canvas dimensions for Fit, including window resizing and inspector visibility or width changes. Fit follows size changes until the user manually pans or zooms. This updates only the camera. Tool shortcuts ignore key events already captured by text inputs. Workspace state (inspector tab and grid visibility) is currently per-session. `src/app/import.rs` routes chosen or dropped files: structures and pictures insert side by side as one Undo step, and `.rsk` drawings open through the save prompt. Window drag events carry no pointer position, so insertions land at the pointer only once the system reports it over the canvas again, otherwise at the view center, and move inward so they stay in view.

The file-open panel is intentionally unfiltered, so opening a supported file does not depend on macOS type registration. Parsing and document validation enforce supported content after selection. Desktop tests observed a delayed Open-button enablement both with and without filters, so the cause is not established; subsequent native reopen checks succeeded.

## Depth appearance and rear opacity

Depth appearance fades atom, bond and fill colors by depth; it is in ReShiki 0.11.0 and later. Rear opacity makes view-occluded ink transparent; it is in ReShiki 0.12.0 and later. Both are stored as `depth_appearance::Scope` entries in `Document::depth_appearance`. Positive Z faces the viewer. See [rear opacity](rear-opacity.md) for what users see.

- **Scopes.** A scope's `atoms` are its RGB owner. Owners span connected components, including coordinate contacts, and `strength`, `weights` and `overrides` drive the RGB fade. Rear alpha targets whole covalent molecules instead: `rear_opacity` is the scope's alpha, and `rear_overrides` keeps per-atom alpha inside a wider RGB owner. A scope with strength zero carries alpha only. `rear_weights` is still written for file compatibility; visibility never reads it.
- **Paint order.** `scene::primitives` builds one `rear_opacity::Paint` from the raw document and its retained XYZ, before any color change. `BondInputs::prepare` (`scene/bond_pass.rs`) then materializes the RGB fade and the canonical canvas colors on a copy, so no stage fades the same paint twice. Ring knockouts build one more `Paint` of that copy, on the first ring stroke. With visibility active, `paint_rear_before_front` moves faded bond ink ahead of opaque bond ink and keeps the order inside each group.
- **View visibility.** `rear_opacity/visibility/` recomputes geometry from the retained XYZ; RGB or frozen paint never acts as a camera. It has two kinds of occluder. Inferred cages (`cage.rs`) are covalent components of 4–256 visible atoms in which every atom has three bonds and the faces close a polyhedron. Occluder ink (`occluder_ink.rs`) is the real projected ink of other objects, from `scene::occlusion_ink`. Ink in a rear-opacity scope becomes transparent only where an occluder in front hides it.
- **Rim exemption.** A cage rim (`rims.rs`) is exposed to its own shell, so the whole rim stays opaque at every setting, including its stroke width, which has no XYZ of its own. An exposed rim atom keeps its original shared bond corner.
- **Budgets and the opaque fallback.** `Visibility::new` checks conservative limits from `visibility/budget.rs` at several points. It estimates the line queries from the managed ink before it searches for cages, stops flattening occluder ink after 100,000 edges, and checks the total work, such as 1,000,000 line queries, once cages, rims and ink are built. The estimates are weights, not an exact operation count. Over any limit it returns no visibility, and the drawing paints opaque exactly as without rear opacity. A flat drawing, with every atom at one depth, also skips visibility and keeps its paint order.
- **Projected double-bond rails.** `scene/projected_bonds.rs` places a double bond's second rail in its retained ring face, projected without rescaling XY. `BondPass` builds one `FaceIndex` per scene; it memoizes faces by stored endpoint order and gives each face search its own budget. Each rail gets opacity intervals along its own centerline.
- **Attachment hit tests.** `scene/attachment_ink.rs` lets a new mechanism-arrow target snap only to ink that is still visible, with the same opacity, crossing and rail clipping as the bond scene. Saved arrow links do not pass this test.

## Native chemistry

Application code forbids `unsafe`. Platform calls stay in `native/windows`, `native/macos` and `native/process` (child-process limits for the naming worker), and the bounded worker allocator stays in `crates/process-heap`. Sanitization and ring failures use `thiserror`, preserving the failed stage and underlying ring error. Tests, examples, build scripts and the Windows helper use `anyhow` for propagation and context. Existing string-error APIs are converted explicitly at those boundaries. Prefer iterator transformations when they clarify data flow; use bounded loops for stateful graph traversal.

The app uses `LocalEngine`, which implements `ChemistryEngine`. Rust handles imports, analysis, cleanup, abbreviations, aromatic display and molecular/drawing exports. It prepares chemistry, computes properties and full CIP labels, lays out molecules and reconstructs drawings; an isolated worker process of the same executable supplies InChI. Blocking tasks keep chemistry off the UI executor. Cached drawing labels never determine chemical identity.

Default builds have no Python worker or uv setup. Ordinary tests exercise the native application. `cargo test --features rdkit-reference` adds the original backend, captured native fixtures and live differential tests. This feature is disabled in release packages and is not a runtime plugin ABI.

`reference/cdx_codec.rs` compares binary output and decoded XML with the Python reference, including native fixtures and every supported property. Its text corpus covers every defined character in the supported legacy codepages. `reference/engine_migration.rs` compares complete responses, stereo identities, isotopes, charges, radicals, figure objects, and rejected queries against RDKit. Malformed binary data must fail before reaching the chemistry backend. The ordinary integration tests use `LocalEngine`, so they exercise the app's migrated path.

`reference/properties.rs` checks all 119 element entries, 3,111 known isotopes, unknown isotope fallbacks, templates, ions, radicals and explicit/implicit hydrogens against RDKit. Formulas and counts must match exactly; masses allow relative error of at most `1e-12` for platform-dependent floating-point operations. JSON parsing preserves full float precision. Reference-bridge tests retain checks that prepared descriptors are not recalculated in Python.

`reference/valence.rs` compares over 278,000 cases with RDKit's strict/intermediate property caches and radical pass: allowed/rejected valences, charges, implicit-H policy, aromatic and partial bonds, dative direction and metal atoms. Graphs reject invalid endpoints, duplicate bonds and excessive size. Intermediate caches tolerate temporary valence excess during normalization; final validation remains strict.

The Rust sanitization pipeline combines functional-group and metal normalization, radical assignment, canonical atom ranking, Kekulé bond assignment, aromaticity, conjugation, hybridization, stereo cleanup and hydrogen restoration. `reference/sanitize.rs` compares 32,987 complete results with RDKit, including valence caches, bond directions, stereo groups, canonical retries and rejected inputs. Intermediate caches follow the reference stage order. A failure returns its stage without changing the input. All native chemistry operations use this preparation pipeline.

`reference/document_preparation.rs` compares complete drawing-to-molecule states against direct RDKit APIs, including stable IDs above the JavaScript integer limit, reordered bonds, mirrored coordinates, stale labels and rejected chemistry. `reference/test_prepared.py` verifies native-state transport and proves that migrated requests skip Python sanitization and stereo perception. The optional reference adapter checks its RDKit valence/ring caches against Rust. Preparation errors stop the request. Document validation indexes endpoints and neighbors, with a 20,000-atom regression.

Kekulé assignment uses bounded, iterative backtracking and preserves bond directions according to the reference rules. `reference/kekulize.rs` compares over 51,000 cases, including rejected graphs, dummy atoms and wedged bonds. It also checks the optional-attempt snapshot used by canonical retries: aromatic flags/orders are restored after a chemical failure, while some direction and hydrogen changes remain. Failed assignment leaves the caller's graph unchanged.

Canonical ranking uses a separate record for atom maps and stereo metadata. `reference/ranking.rs` compares over 45,000 rank vectors against RDKit, including symmetry classes, different ring caches, tetrahedral and bond stereo, stereo groups, and atom permutations. It also checks over 10,000 bond assignments using Rust-generated ranks. Ranking reads existing stereo annotations; perception is a separate pass.

`crates/chemistry/src/smiles/write.rs` prepares components, ranks atoms, assigns Kekulé bonds when requested, adjusts stereo and assembles plain SMILES. Prepared molecular imports, analysis and SMILES exports use it asynchronously; transport tests forbid the native SMILES writer on these paths. Independent tests compare full strings and atom/bond order, including mixtures, maps, ring stereo, coordination winding and custom symbols. Malformed states, 20,000 disconnected atoms and combined text limits have separate checks.

Small C++ reference fixtures verify the shared ring/traversal sorting and its heap fallback on each platform. Regenerate them with `reference/smiles_sort_reference.cpp` and the platform compiler; production uses only the checked Rust implementation. Adapted standard-library algorithms and their licenses are recorded in `licenses/stdlib/`, which release packages include.

Stereo assignment uses a separate atom-priority pass. `reference/cip_ranking.rs` compares 43,190 cases with RDKit's direct legacy ranking API, including isotope priorities, explicit zero and maximum maps, hydrogens, special bonds and reordered atoms. Rust bounds refinement work and storage, handles empty graphs, and uses dynamic neighbor lists. These priorities support the reference's legacy stereo perception; they do not replace full CIP labeling. `reference/perception.rs` compares 35,783 complete assignment results, including legacy R/S and E/Z labels, ring-stereo relationships, cached properties, unknown stereo and group repair. Errors leave inputs unchanged; group expansion and iterative refinement are bounded.

Metal cleanup converts eligible single bonds to donor→metal bonds while preserving atom, stereo and display metadata. `reference/organometallic.rs` checks 25,209 cases, including multi-metal complexes, existing dative bonds and rejected inputs. It also compares the exact cycle order of the bounded, iterative fast ring pass used by ranking before full ring perception.

`reference/electronic.rs` compares pi-electron counts, conjugation and hybridization in 59,631 cases. Coverage includes all elements, radicals, charges, special bonds, coordination geometry, explicit hydrogens and the NCI molecule sample. These passes return separate annotations without changing the graph; stereo perception follows them.

`reference/stereo.rs` compares atom-chirality and atropisomer cleanup in 47,835 cases, including coordination permutations, ring-size limits and stereo-group membership and IDs. Cleanup repairs existing annotations; it does not assign absolute configurations. Invalid metadata and excessive work or group expansion return errors without changing the input.

`reference/drawn_stereo.rs` compares 62,137 cases for atom winding from wedge/hash geometry and cis/trans annotations from up/down bond directions. Coverage includes conflicting wedges, near-linear or overlapping bonds, mirrored/scaled drawings, existing tags and explicit-H promotion. Coordinate validation and batched atom-cache updates keep errors atomic and large drawings bounded.

`reference/bond_geometry.rs` compares 39,661 cases for double-bond direction assignment from coordinates or existing stereo tags. Coverage includes conjugated chains, ring-size limits, unknown stereo, near-linear geometry, atom/bond permutations and absent conformers. Propagation uses a bounded stack; invalid coordinates or exhausted limits return errors without changing the input. Legacy R/S and E/Z assignment uses the separate perception pass.

Rust can select and orient wedge/hash bonds from existing stereo tags, including optional second wedges and 2D/3D atropisomers. `reference/wedging.rs` compares complete states with RDKit's molecule and single-bond APIs. Rust preserves atom order when selection priorities tie; tests verify those choices against native outputs under equal-priority atom permutations. Ring membership must remain identical, and Rust must preserve the original cache order. Invalid inputs or exhausted work limits leave the caller's state unchanged. The application now uses it when reconstructing analyzed drawings and molecular exports.

`reference/drawing_output.rs` compares 12,218 drawing outputs with direct native bond-assignment APIs and the original document converter. Canonical ranking matches the reference’s drawing defaults. Full CIP results include changed bond-stereo controls as well as labels; reconstruction maps both back to stable IDs. Styles, groups, captions and aromatic circles are retained, and the result remains one undoable edit. Imports, cleanup and figure interchange use the same checked Rust drawing conversion.

Aromatic-circle toggles prepare both chemical states, update selected rings and verify matching canonical SMILES in Rust before returning the edit. `engine::native_aromatic` completes analysis through the isolated InChI worker without starting Python. Independent tests compare complete responses and rejected inputs, including partial fused-ring selections, heterocycle hydrogens, stereo and undo/redo. Guarded routing checks cancellation, concurrent snapshots and helper failures without fallback.

Abbreviation detection and replacement run in Rust from one catalog file, `crates/model/src/chemistry/abbreviations/catalog.json`. Each entry's SMILES is parsed at load time to derive its matching query and formula, and laid out with the native depiction port for its template coordinates. Matching preserves native priority, overlap and selection rules; replacement retains attachment IDs, styles and group/reaction membership. A frozen per-platform RDKit capture of the original 62 groups guards their layouts. Independent tests compare original-worker results, complete app responses, concurrent requests, numeric transport and undo. Final drawing reconstruction and full CIP labeling use Rust; the isolated InChI worker supplies InChI. The adapted coordinate norm and its license are recorded in `licenses/cpython/`.

MOL export writes V2000/V3000 in Rust from the prepared molecule. Dative bonds, large graphs and large coordinates select V3000 automatically. `reference/molfile.rs` compares exact native output, including wedge endpoint reversals, unspecified double bonds, isotope/charge/radical records and format boundaries. Engine tests reimport exported structures and compare identities, atom maps, masses and stereo. They also preserve the existing rejection of generic R atoms as queries on MOL import.

MOL import runs in Rust on a blocking task. The reader in `crates/io/src/chemistry/molfile/read/` parses V2000/V3000, applies file-specific valence/hydrogen rules, then runs 2D/3D stereo and sanitization. Drawing reconstruction retains file coordinates, generated wedge directions and dummy labels. File annotations retain the conformer dimension and signed attachment values, including explicitly stored zero. `reference/molfile_import.rs` compares complete molecular states with direct RDKit imports, including legacy property records, enhanced stereo collections, substance groups, atom maps, continuation lines, templates and malformed files. The optional `RESHIKI_RDKIT_SOURCE` path adds the reference source's MOL and substance-group fixtures. Substance-group SMARTS uses Rust syntax validation, including CX extensions: valid queries exceed the editable contract, while invalid query text is ignored as in the native reader.

`reference/molfile_drawing.rs` compares complete drawing states and editable output, including full CIP results. `reference/engine_migration.rs` compares complete import responses. Default MOL imports complete without Python. The retained bridge tests forbid native parsing, sanitization, stereo assignment, Kekulé/wedge generation and drawing conversion for prepared imports. Its signed ring-stereo transport uses a bounded, version-checked native binary property block because Python has no atomic integer-vector setter.

Substance-group parsing checks members, crossing bonds, attachments, defaults and continued data fields. Chemistry-changing records run in native order, including charge/H overrides, aromatic H counts and conversion of unspecified/query bonds to coordinate bonds. Coordinate-bond replacement adjusts explicit H on both endpoints and clears bond stereo. Tests cover both accepted and rejected records, native numeric-field behavior, nonsequential bookmarks and bounded default expansion.

`reference/smarts.rs` compares query acceptance and atom counts with direct RDKit parsing. Coverage includes recursive and Boolean queries, range expressions, isotope/H/charge syntax, chiral permutations, ring closures, names and UTF-8 boundaries. CX extensions validate coordinates, escaped labels, atom properties, bond references, stereo groups, link nodes, variable attachments and substance groups. Branches and negation chains are iterative; recursive queries, input size and atom counts have explicit limits. The same query corpus also runs through native MOL substance groups, without skipped parser cases. This validator does not replace query matching or the SMILES importer.

SMILES import runs in Rust through `crates/chemistry/src/smiles/`: names, CX annotations, hydrogen removal, sanitization and stereo perception. Shared CX parsing retains annotation order, coordinates, labels, radicals, bond changes, stereo groups and protected substance-group members. Independent tests compare prepared states and conformers; a separate suite checks coordinate values bit for bit, including each platform's hexadecimal and underflow rules. Optional numeric annotations fail only when an operation reads them, matching native import behavior.

RXN and reaction SMILES export run in Rust through `crates/io/src/chemistry/reaction.rs`. It preserves explicit participant roles, coefficients, maps, stereo and aromatic bond types without changing the drawing. Differential tests compare complete files and rejected inputs; a backend-free test verifies the runtime path. Reaction SMILES sorts participants within each role, groups disconnected components and expands coefficients. Backend-free tests cover both reaction export formats.

RXN and reaction SMILES import use Rust readers and canvas assembly, preserving participant roles, stereo, label spacing, reagent rows and separators. Placement keeps full precision until the final canvas coordinates. Rust labels each participant, prepares the combined analysis graph after labeling and writes its SMILES. RXN and reaction SMILES use the native response path; the Rust solver supplies missing participant layouts. Complete-response and concurrent-import tests compare the original importer. Incomplete labels or invalid coordinates cannot publish a partial drawing.

Reaction SMILES import retains explicit H, grouped reactants, disconnected agents and global CX annotations. Existing conformers keep their coordinates and dimension; the Rust solver lays out only participants without coordinates. CX attachment markers guide Rust wedge selection. Untyped CX properties travel separately to the layout pass, preserving numeric conversion errors only when the native algorithm reads those values.

Rust supplies 2D coordinates, labels and the editable drawing; the isolated InChI worker supplies InChI. CX coordinates used for perception can be nonfinite; they never enter drawing APIs. Complete-response tests include templates, isotope/stereo cases, names, malformed input and optional source fixtures. Concurrent imports keep separate layout results. Transport tests forbid native parsing, H removal, sanitization, Kekulé/wedge generation and drawing conversion. Explicit zero atom maps and independent atom/bond aromatic flags survive transport.

Hydrogen removal preserves isotope H, protected group members, winding and double-bond controls. Invalid properties on removed hydrogens are discarded before validation. Query bonds removed with hydrogen are allowed; surviving queries are rejected. Malformed metadata and excessive work return typed errors without changing input.

Axial stereo detection runs before sanitization in the MOL reader. `reference/atropisomer.rs` compares exact tags with native unsanitized MOL and CXSMILES reads, including 2D, 3D and absent coordinates, aromatic rings, reversed bonds and ambiguous geometry. Candidate collection and neighbor access stay linear for high-degree graphs. Errors leave the caller unchanged. The reader retains the editable document's existing rejection of surviving axial stereo tags.

`reference/spatial_stereo.rs` checks 3D atom perception against the native geometry API, including tetrahedral winding, all 53 coordination permutations, geometric tolerances, existing tags and annotation presence. The pass preserves connectivity and hydrogen counts, clears the stereo cache only for 3D conformers, and returns typed errors without editing inputs. The MOL reader uses it before sanitization; unsupported coordination classes retain the existing import rejection.

Editable drawing export runs in `crates/io/src/exchange/drawing/`, including rich text, labels, marks, arrows, graphics, pictures, bond crossings, nested groups and abbreviations. `reference/drawing_exchange.rs` compares XML object ownership, attributes and text against the original Python writer; PNG comparisons check decoded pixels. Engine tests also compare binary exports byte for byte and reimport chemical identities. The writer preserves unsupported-feature rejections and bounds XML size, object count, nesting and graph work.

The Rust CDXML libraries expand abbreviations, normalize bond depictions and parse molecular fragments without inventing sanitization caches. Separate readers preserve drawing styles, rich text, bond appearance, marks, labels, arrows, graphics and groups. They retain native f64 values and source-object identities until checked document conversion. Chemical preparation combines fragments, preserves coordinate scales and runs sanitization and stereo perception in native stage order. Scene assembly preserves object ownership, aromatic circles and final appearance. Differential tests compare original scenes and the actual JSON-to-document transport. Default CDXML/CDX imports complete natively, generating molecular coordinates when absent.

Rust computes symmetric SSSR rings with iterative, bounded searches. `reference/ring_perception.rs` compares ordered atoms and bonds against native RDKit, including NCI molecules, permutations, dense graphs and disconnected components. Greedy pruning uses the pinned platform's equal-key sorting policy from `crates/chemistry/src/native_order/`, shared with SMILES traversal. Ring analysis and preparation no longer fall back to Python or accept transported ring overrides.

Runtime analysis derives InChIKeys in `crates/chemistry/src/inchi/key.rs` using safe Rust and software SHA-256. Rust derives the key from the generated InChI; export reuses that string. Empty or unsupported identifiers retain empty keys. Independent tests compare native keys and error codes, layers and platform integer behavior; an optional pinned 1,181-entry corpus adds chemical coverage. Malformed non-ASCII starts use deterministic ASCII validation; tests report native locale differences separately.

`chemistry::inchi::input` prepares owned atom, bond and stereo records for InChI 1.07.3 without FFI. Independent captures compare the exact arrays passed by the original adapter to its generator, including conformer absence, isotope/H handling and ordered bonds. Invalid indices and undefined native stereo inputs return typed errors without changing the source molecule.

`chemistry::inchi::generator` relaunches the application with `--inchi-worker`, using the pinned `cosmolkit-inchi` Cargo dependency. ReShiki supplies its existing molecular property cache, sanitization and stereo routines through toolkit traits. Bounded versioned frames carry molecular graphs and diagnostics; the dependency does not expose raw C records or warning masks. Deadlines and cancellation terminate the helper process. A Rust allocator bounds live allocations during the operation, including the adapter and response; this is not an RSS or stack limit. Runtime routes use `current_exe()`; an absolute developer override can select a transport-test executable. Release packages contain exactly one application executable. Build metadata records the locked InChI dependency, and checks validate architecture and worker operations after extraction or installation. See `tools/inchi-helper/README.md`.

Runtime InChI reconstruction runs inside the Rust helper through the dependency's molecular API and ReShiki toolkit callbacks. The async reader returns the reconstructed state, diagnostics and unspecified-bond identities; it compares 1,848 original imports and complete reconstruction outcomes, plus 25 text boundaries. `chemistry::inchi::output` supplies the sanitization and hydrogen-removal callbacks and retains reconstruction from detached C records for independent development reference checks, including all 19 reachable cleanup rules. Those C records do not cross the runtime helper protocol. Native InChI imports then use the Rust layout and drawing pipeline.

Cargo builds the app and its worker modes from one locked dependency graph. The fork's compatibility fixes are part of the pinned Git revision; the build does not patch a private C source copy. A separate development-only executable shares the worker implementation for independent transport and fault tests; it is never copied into releases.

`engine::native_response` builds the default Analyze, abbreviation and ordinary molecular export responses from immutable prepared state. Blocking chemistry runs off the async executor; helper cancellation cannot publish an edit. `engine::native_import` completes imports with existing coordinates or generated Rust layouts. Differential tests compare complete original-worker responses and rejected inputs. Isolated subprocess tests forbid Python startup on native routes and check helper failures without fallback. Reference-feature tests retain the independent original backend.

`chemistry::depict` performs deterministic 2D layout: ring construction, neighbor attachment, stereo seeds, templates, fragment expansion, collision correction and final placement. Complete layouts match the original public API on Linux x64, macOS ARM64 and Windows x64, with exact coordinate bits and unchanged chemical state. Each stage also has independent native fixtures. `reference/depict_numeric.md` records platform arithmetic, and `reference/depict_pipeline.md` records the complete-layout contract. Those local comparisons do not establish ARM Windows/Linux graphical acceptance.

Coordination seeds use the current request's bond length. Native static caches retain the first requested length; the Rust solver instead matches a fresh native call, so an earlier drawing cannot change a later drawing's style. Forward/reverse requests and failed custom-length requests check this policy.

`chemistry::cleanup` prepares selected components and merges checked layout results while preserving fixed atoms, remote bonds, styles and chemical/visible stereo. Independent tests compare 308 complete responses and 22 rejections on all three local platforms. The default engine completes cleanup through this pipeline. Native assertion-only warnings use truthful Rust diagnostics; helper failures cannot become partial success.

Full CIP migration includes read-only molecular contexts and lazy graph expansion in `stereo::cip`. Persistent visit maps share unchanged branches, avoiding a molecule-sized history copy per node. Expansion, re-rooting and searches are iterative and bounded; failed expansion cannot supply a partial labeling graph.

Sequence comparison and priority sorting use an explicit work stack with bounded storage and shared iteration limits. All nine full CIP sequence rules, tetrahedral/double-bond/axial configurations and auxiliary descriptor assignment are implemented. The complete labeling pass returns detached state and neighbor orders, bounds retained graph storage, and leaves input unchanged on failure. Native-reference tests cover complete labels, selection, stale codes, dependent centers and rejected inputs. Drawing tests compare Rust labels and finished documents with the original converter. All native imports, analysis, exports and cleanup use Rust labeling. Reaction imports label each participant before canvas assembly. The optional reference bridge retains `local_cip` validation tests, including concurrent requests and undo; invalid label responses fail before publishing a drawing.

`reference/cip_molecule.rs` and `reference/cip_digraph.rs` compare recorded native API results, including partial Kekulé failures, cache-access order, lazy expansion, re-rooting, duplicate fractions, isotope mass bits and edge ordering. Input hashes guard against stale reference data. Duplicate visit distances retain the native C `char` byte representation and use the target platform's signedness.

Regenerate CIP fixtures on macOS with `uv run --locked python reference/build_cip_molecule_oracle.py --rdkit-source ~/dev/rdkit`, adding `--component digraph`, `--component rules`, `--component pairing` or `--component configuration` for graph and comparison fixtures. This optional helper links the pinned wheel's native library and requires Apple clang and Boost headers. Fixture replay uses the reference Python environment but needs no C++ compiler or source checkout. `reference/cip_labeling.rs` also calls the public labeling API directly. Set `RESHIKI_CIP_VALIDATION_SUITE` to the pinned source's `Code/GraphMol/test_data/compounds.smi` to include its 300-compound suite; the test checks its checksum.

Rust computes logP, polar surface area and hydrogen donor/acceptor counts using pinned descriptor rules and a bounded query matcher. `reference/descriptors.rs` compares totals, atom contributions and Crippen type assignments against RDKit, including explicit hydrogens, atom permutations, special bonds and the reference's 1,000-match limit. Floating-point results allow relative error of at most `1e-12`; counts and types match exactly. The suite also checks molar refractivity and sulfur/phosphorus surface contributions, although the UI does not expose them.

Embedded PNG, TIFF, JPEG, GIF and BMP normalization runs in Rust. Imports validate and decode picture payloads before exposing a document. Export supplies prepared PNG data, including lossless row reversal for reflected pictures. Image work runs off the UI thread with the existing size and document budgets. Lossless pixels must match the Pillow reference exactly; JPEG color channels may differ by at most 2/255 between decoders. Alpha must match exactly.

The documented `vendor/zune-jpeg` patch preserves libjpeg's small-component chroma sampling and integer transform precision. Independent JPEG tests compare image edges, sampling ratios, orientation, opacity and scalar/SIMD output. Release packages retain its original licenses, IJG terms and modification notice.

Pillow is a development-only image reference. The minimal RDKit reference-environment check also runs without Pillow; neither is included in release packages. TIFF coverage includes palettes, grayscale alpha, integer/float samples, compression, EXIF orientation and associated alpha. Planar 16-bit fixtures also check known source samples against Pillow's libtiff reader, avoiding a channel-decoding bug in its default raw reader.

Regenerate codec constants and codepage tables with `uv run --locked python scripts/regenerate_cdx_rust_schema.py`, followed by `cargo fmt --all`. The generator uses the checked Python schema and standard-library codecs; released applications read only compiled Rust constants and tables.

Element, isotope, allowed-valence and outer-electron data come from RDKit `Release_2026_03_6`. Regenerate them with `uv run --locked python scripts/regenerate_atomic_data.py --rdkit-source ~/dev/rdkit`, then `cargo fmt --all`. The generator verifies the source checksum and compares every entry with installed RDKit. Its BSD license and attribution are in `licenses/rdkit/` and are included in release packages.

Regenerate descriptor rules with `uv run --locked python scripts/regenerate_descriptor_data.py --rdkit-source ~/dev/rdkit`, then `bun run --bun oxfmt crates/chemistry/src/descriptor_data.json`. The generator verifies source checksums and compiles the fixed queries into checked-in data. The application reads that data without invoking Python or parsing SMARTS.

Packages contain no Python interpreter, worker project, uv environment or companion executables. Archive and installer checks exercise the relocated application with Python/uv unavailable and reject chemistry-cache creation. macOS bundles retain their icon, license notices and signing. Windows upgrades retire the app-owned old chemistry directory, InChI helper and expanded license trees, preserving user data and caches. The `Licenses` directory contains the four project license/notice files and `THIRD-PARTY-NOTICES.txt`; indexed SHA-256 references share only byte-identical texts, preserving every dependency declaration, nested notice and source attribution.

## Native clipboard

On macOS, explicit Copy/Paste relaunches the same executable with `--clipboard-worker`, using Rust AppKit bindings with JSON on stdin/stdout and base64 representations. The helper prepares one item with a private ReShiki document, supported editable binary drawing data and PDF/PNG/SVG alternatives. Copy Image omits the editable structure and adds an embedded raster drawing object with physical bounds for readers that ignore PNG resolution metadata. The helper does not monitor clipboard changes or read previous contents during Copy.

The app accepts `format: "cdx"` with base64 input/output. `LocalEngine` converts it in Rust and applies the same native import path as CDXML. The binary codec bounds input, output, nesting, object count and property count. Unsupported object properties and query predicates return errors. The optional reference worker retains its original CDX path as a test oracle.

Clipboard tasks capture document epoch and revision. Cut removes the captured selection only after a successful write and only if the drawing remains unchanged. Paste validates and inserts in one Undo step, rejecting stale results. Rendering uses a snapshot and runs off the UI thread. The same entry point works in development and installed packages; no Swift compiler or separate clipboard binary is needed. Printing uses `--print-worker` with a bounded PDF snapshot and Rust AppKit/PDFKit bindings, keeping native dialogs off the editor event loop. Windows uses its native clipboard and editable Office object bridge. Linux exchanges drawings, images and explicit text formats through X11 or the focused Wayland window's clipboard connection. Ordinary Copy supplies no chemical plain text on any platform.

Chemical abbreviations store presentation metadata over the complete atom/bond graph. Cleanup requires selected atoms in the desktop. Rust splits connected components, redraws the requested atoms/molecules, pins unselected atoms and preserves each component’s placement. Cleanup results remain transient until Apply; a revision and document epoch reject stale previews, and Apply commits one history step. Template connection preview and insertion use the same pure geometry operation.

Aromatic circles are derived from closed cycles of aromatic bonds (order 4), without a detached graphic in the native model. Rust changes selected rings and checks their chemical identity before publishing the edit. Editable exchange emits aromatic bonds and owned circle graphics; import recognizes these circles while retaining independent ovals. Contextual atom/bond shortcuts run only after focused widgets have ignored a key.

## Reviewed assistant proposals

The optional Codex panel uses a local app-server child with structured output. The child has an isolated temporary working directory, shell and external integrations disabled, bounded JSONL transport, cancellation, and a turn timeout. Finder launches receive known executable search directories without evaluating shell startup files. ReShiki imports proposed SMILES through the native chemistry engine and builds typed drawing objects with current styles. Chat and previews are transient; document epoch/revision checks protect Apply, which commits one ordinary history entry. The assistant never owns the live document.

Generation emits public activity, composition plans, actual structure counts and retained draft previews independently of the final structured proposal. Measured layouts support rows, central examples, grids and shared-reactant branches. The same app-server session then reviews overview and close-up images with editable object data. Corrections are validated and rendered again; only an exact final draft with no unresolved findings can pass the automatic-acceptance gate. Cancellation retains completed previews, and closing the panel leaves the job running. Preview geometry is cached independently of the activity timer. See [assistant flows and limits](assistant.md).

Bond Z order is presentation metadata. A bounded sweep detects unconnected crossings, then clips lower-bond line/polygon geometry. Canvas and exports share these primitives. Elbow arrows use two line segments and retain a movable corner through native and supported editable interchange.

## Generic structures

`Document::generic` holds element and group alternatives, link nodes and repeating units; variable attachment points are atom data. They stay drawing data until **Expand generic structures** materializes them, and expansion never changes the source. [Generic structures](generic-structures.md) lists the supported definitions and limits. The module root is `crates/model/src/generic.rs`; its submodules live in `crates/model/src/generic/`.

| Module                                              | Job                                                                                               |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `generic.rs`                                        | Typed definitions, `Limits`, notices and the public API                                           |
| `definitions.rs`                                    | `validate`, `prune` and `selection`                                                               |
| `authoring.rs`                                      | Label parsing (`alternative`, `apply_label`) and repeating-unit definitions                       |
| `guards.rs`, `references.rs`                        | Whether an edit touches generic data, and inherited references of changed components              |
| `expansion.rs`, `materialize.rs`                    | Bounded finite enumeration and concrete fragment insertion                                        |
| `variable.rs`, `depiction.rs`                       | Placement of variable substituents and generated link and repeat parts                            |
| `clearance.rs`, `ink.rs`, `certify.rs`              | Clearance certificates, ink measurement and checks that preserved data and translations are exact |
| `symmetry.rs`                                       | Exact chemical equivalence of completed products, for duplicates                                  |
| `budget.rs`, `prepared.rs`                          | The work allowance and its cost model; the prepaid input and the sealed result                    |
| `limits.rs`, `errors.rs`, `graph.rs`, `geometry.rs` | Shared bounds (also used by reshiki-io), refusal text, ordered adjacency and bounds helpers       |
| `probe.rs`                                          | Stage counters for tests; a no-op in production builds                                            |

The budget rule is: charge before you clone or scan; one allowance per request; never reset. A request gets one `Work` allowance (`Limits::default().max_work`, 50,000,000 units). Every stage debits that counter before it copies a document, scans ink or measures labels, and no stage starts a fresh budget. Running out rejects the whole request; nothing is installed.

Insertion has two phases, so the UI thread never runs the expansion (`src/app/generic.rs`):

1. `generic::prepare_expansion_input` borrows the live drawing and selection. It checks the limits and pays the presentation and selection costs before it copies the snapshot into an owned `ExpansionInput`.
2. `generic::expand_prepared` runs on a blocking task with the remaining allowance. It enumerates, lays out, measures and validates the products and returns a `PreparedExpansion`.
3. `transaction::take_expansion` moves the document out of the result before the app opens a tab.
4. `transaction::install_expansion` installs it into that new, empty tab. It does not run reconcile again, because the result is already validated.

`PreparedExpansion` holds its document in a single-use `Arc<Mutex<Option<Document>>>`. The result travels inside an Iced message, and messages are cloned. A clone copies only the `Arc`, and `take` empties the `Option` once, so a cloned or replayed message cannot install the drawing twice, and a failed take opens no tab. The app accepts a result only for its pending tab and file epoch. It reports a source tab that was closed or replaced, and discards a result that arrives while a blocking modal is open.

The io boundary:

- CDXML and CDX import: `crates/io/src/chemistry/cdxml/generic.rs` detaches typed generic data before the molecular parser runs and restores references through source object IDs.
- CDXML and CDX export: `crates/io/src/exchange/drawing/generic.rs` writes ChemDraw generic properties on their atom and bracket IDs.
- MOL: the reader in `crates/io/src/chemistry/molfile/read/` fills `Generic` from substance groups, and `crates/io/src/chemistry/molfile/generic.rs` writes V3000 generic records.
- `crates/io/src/engine/native_import.rs` adds the generic notice to imports that contain generic structures.

## NMR prediction

**Properties → Molecular properties → Predict NMR…**, or **Predict → NMR** in the inspector, predicts ¹H or ¹³C shifts for one molecule from an offline index. [NMR prediction](nmr-prediction.md) describes the method and its limits.

| Layer             | Files                                                             | Job                                                                                                                                             |
| ----------------- | ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Encoder           | `crates/chemistry/src/nmr.rs`                                     | Versioned HOSE-style spheres (`reshiki-hose-v1`) from radius 4 down to 2, median lookup, and a 3-second work deadline (`Environment::deadline`) |
| Adapter and index | `crates/io/src/nmr.rs`                                            | `prepare` (one whole molecule, at most 128 atoms), `fingerprint`, the bundled `data/nmr/index.tsv`, `predict` and TSV export                    |
| Dataset           | `crates/io/src/nmr/dataset.rs`, `crates/io/examples/nmr_index.rs` | The nmrshiftdb2 transform that builds the index offline. The app never calls it; `scripts/regenerate_nmr.py` runs it.                           |
| Job               | `src/app/nmr/worker.rs`                                           | One prediction at a time per process, on a blocking task with a 6-second outer timeout                                                          |
| Panel             | `src/app/nmr.rs`, `src/app/nmr/`                                  | Per-tab state, the floating palette, the docked report, the shift plot, styles and accessible text                                              |
| Labels            | `src/canvas/nmr.rs`                                               | Session-only atom and shift labels; they never enter the document, the scene or exports                                                         |

Results are gated by chemical identity. `nmr::fingerprint` hashes the stored chemistry and stable IDs of the molecule: atoms with their element, charge, isotope, radical, hydrogen, aromatic and stereo fields, and bonds with their order, stereo and display class. Position, projection paint, labels, atom-map numbers and collapsed abbreviations do not change it, and generic dependencies are refused. A job key holds the request serial, file epoch, atom IDs and fingerprint. A finished job is accepted only for the pending key, and only while the file epoch and fingerprint still match. After every change, including Undo and Redo, the app checks the pending job and the shown report again; a mismatch clears both and shows "Molecular chemistry changed; run prediction again".

## Chemical names

Chemical names turn a name into a structure (**Import → Name**) and a structure into a name (**Show chemical name** and the **Chemical names…** inspector), offline. [Chemical names](chemical-naming-rust.md) describes the behavior and its limits.

- **Rules.** `src/naming/rules.rs` and `src/naming/rules/` hold ReShiki's own nomenclature profile, "ReShiki organic rules 1". The rules work on prepared `reshiki::chemistry` state from reshiki-io and do not use OPSIN. They name molecules of 1–64 heavy atoms within a 200,000-unit work budget.
- **OPSIN boundary.** Only `src/naming/worker.rs` calls the `opsin` crate: `Parser::parse` with strict options, then semantic CXSMILES. `src/naming/catalog.rs` reads only its upstream version and commit. The worker reports the protocol, port version, upstream commit, resource fingerprint and options with every parse, and the client rejects any mismatch.
- **Worker client.** `src/naming/client.rs` runs each request in a fresh `--naming-worker` child under the [naming worker limits](runtime-safety.md#naming-worker). `local.rs` parses a name. `reverse.rs` generates a name, parses it again in a second worker and returns it only when the result has the same canonical identity. Each direction runs one request at a time.
- **Canonical identity.** `naming::canonical_smiles` and the caption identity in `crates/model/src/molecule_names.rs` share `molecule_names::canonical_identity`: canonical isomeric SMILES with isotope, charge and specified stereo, without atom maps or atom order. Both use `MAX_NAMING_ATOMS` (512) and `MAX_SMILES_BYTES` (32,768) and keep their own error text. The alias catalog is keyed by the same identity.
- **Alias catalog.** `src/naming/catalog.rs` holds the bundled offline alias index: `catalog/data.json`, with its pinned provenance in `provenance.json` and its sources in `README.md`. It is keyed by `naming::canonical_smiles`, loaded once through `OnceLock` and fails closed; see [runtime safety](runtime-safety.md#alias-catalog) for its bounds. The data terms (OPSIN MIT, ChEBI CC BY 4.0) are in `licenses/name-aliases/` and listed in `NOTICE`.
- **App.** `src/app/naming.rs` and `src/app/naming/` hold the inspector, the Import dock name field and name captions. `src/app/naming/aliases.rs` holds the alias and generated-name controls that **Import → Name → Details** and the Chemical names inspector share.

## Agent API (experimental)

The agent API is in ReShiki 0.12.0 and later. `reshiki --mcp` serves the [agent API](agent-api.md) to an MCP client over standard input and output, and `reshiki --cli` runs the same operations once from the command line. Both are modes of the single `reshiki` executable, chosen in `src/main.rs` after the worker modes and before Office registration and the GUI. Only a first argument of exactly `--mcp` or `--cli` selects them.

```mermaid
flowchart LR
    Client[MCP client] -- stdin / stdout --> Framing[reshiki-mcp framing: lines, admission, cancellation]
    Framing --> Protocol[rmcp protocol and ToolHost bridge]
    CLI[reshiki --cli] --> Host
    Protocol --> Host[ToolHost: HeadlessHost; AppHost in P2]
    Host --> Ops[ops: executor, session documents, operations]
    Ops --> Engine[LocalEngine and export]
    Ops --> Access[access: folder grants]
    Access --> Dirs[Granted directory handles]
```

- The framing reads one bounded JSON-RPC message per line on its own thread, answers malformed lines itself, admits requests and stops reading while too many are outstanding. Its writer drops the responses of cancelled requests.
- The protocol layer pins rmcp 3.5.1 behind ReShiki's own transport. It serves MCP 2026-07-28, 2025-11-25 and 2025-06-18 and maps operation results to MCP content: `structuredContent`, images and inline embedded resources.
- `HeadlessHost` owns session documents in memory. Its executor bounds concurrency, queueing and deadlines; every edit is one atomic store commit. P2 adds an `AppHost` for drawings open in the app.
- Operations call the same `LocalEngine`, composition and export code as the app. Only `file_open` and `file_save` touch files, through `access`, which resolves each request against directory handles opened once at startup.
- `reshiki --cli` runs each command on an in-process `HeadlessHost` through the same `ToolHost::call`, and reads and writes its command-line paths with the user's own permissions.
- `src/cli/commands.rs` holds one table of commands and their options. It drives the parser in `src/cli/args.rs`, the help text and `reshiki --cli help --json`. `src/cli/reference.rs` renders [the agent reference](agent-reference.md) from the help, the tool catalog and the schemas (`cargo run --example agent_reference -- docs/agent-reference.md`), and a test fails when the committed page is out of date. `src/cli/skill.rs` writes the [Agent Skill](agent-skill.md); `src/cli/io.rs` is the only module in `src/cli` that touches the filesystem.
- The experimental API stays out of the published Rust API: `reshiki_agent::ops` is `#[doc(hidden)]` and `reshiki-mcp` is an unpublished crate.

Budgets, containment and residual risks are recorded in [runtime safety](runtime-safety.md#agent-api-p1-experimental).
