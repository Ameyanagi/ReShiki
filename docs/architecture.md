# Architecture

Moruno owns its editable document in Rust. RDKit is a local computation service; its Python objects are never serialized into native files.

```mermaid
flowchart LR
    UI[Iced controls and canvas] --> App[Messages, history, selection]
    App <--> Doc[Rust document]
    Doc --> Scene[Vector scene]
    Scene --> Canvas[Iced geometry]
    Scene --> SVG[SVG export]
    App --> Contract[ChemistryEngine / protocol v1]
    Contract <--> Worker[Python process]
    Worker --> RDKit[RDKit]
    Contract -. future .-> Rust[Pure Rust engine]
```

## Modules

| File | Responsibility |
| --- | --- |
| `src/document.rs` | Atoms, bonds, text, arrows, stable object IDs, validation, undo/redo snapshots |
| `src/scene.rs` | Toolkit-independent lines, polygons, labels and SVG serialization |
| `src/canvas.rs` | Hit testing, pointer gestures, snapping, camera and previews |
| `src/app.rs` | Desktop controls, asynchronous requests, selection and file workflows |
| `src/engine.rs` | Chemistry interface, worker lifecycle, timeout and response validation |
| `src/storage.rs` | Write complete files beside the destination, then atomically replace |
| `engine/worker.py` | Molecular parsing, sanitization, descriptors, depiction and exchange formats |

Document coordinates use screen-style positive-down Y, with 28 points per RDKit coordinate unit. The default single bond is 42 points. The camera never changes stored coordinates. Native documents are versioned JSON; history and camera are session state.

Atoms retain formal charge, isotope, explicit-H count, implicit-H policy, map number and tetrahedral winding. Winding refers to an explicit ordered list of stable neighbor IDs. The worker compensates for permutation when constructing a toolkit molecule, preventing array reordering from reversing a stereocenter. Double-bond stereo stores its reference atoms separately. Topology edits invalidate affected stereo and derived hydrogen labels; the next structure check recomputes chemistry.

## Worker protocol

One JSON object per line on stdin/stdout. Diagnostics use stderr. Requests and responses include a numeric request ID; requests also have `protocol: 1`. Supported operations are `import`, `analyze`, `clean`, and `export`.

```json
{"id":1,"protocol":1,"operation":"import","format":"smiles","text":"CCO"}
```

Successful responses contain `ok: true` and `result` with the engine version, document and analysis, or an exported string. Failures contain `ok: false` and an error. One persistent worker processes serialized requests. A failed, exited or timed-out worker is dropped and restarted for the next request. The timeout is 30 seconds.

Iced tasks keep chemistry work off the UI thread. A document revision prevents late analysis/import/cleanup from replacing newer edits. Export uses the snapshot requested. Save records the exact snapshot written and a document epoch, so a late save cannot mark later edits as saved or redirect another document's path.

## Rendering and interaction

Both the canvas and SVG consume the same vector primitives. Canvas text is emitted as glyph outlines to maintain drawing order. SVG retains editable text and depends on compatible fonts in the viewer. Label placement and collision handling are still approximate.

Pointer motion is read from each event, rather than only Iced's latest cursor snapshot. This matters when multiple move/press/release events arrive in a single batch. The desktop drag test found this issue; a regression test now reproduces that event sequence.

The file-open panel is intentionally unfiltered. During macOS testing, native extension filters left valid files selected with the Open button disabled. Parsing and document validation enforce supported content after selection.

## Pure Rust migration

The current app instantiates `PythonEngine`, which implements `ChemistryEngine`. A future backend implements the same request/response contract, and backend construction in `App` is changed. The trait uses a statically dispatched async future; it is not a runtime plugin ABI.

First move graph checks, formula/mass and simple descriptors into Rust. Then add parsers, aromaticity, stereochemistry and canonical identifiers with a reference corpus. Move 2D coordinate generation separately from depiction. Keep Python as a selectable verification backend until stereo, charges, isotopes, salts and interchange pass differential tests. Retiring the Python runtime is a later packaging milestone, not an existing capability.
