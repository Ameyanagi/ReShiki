# Runtime safety audit — 2026-09-20

The application and library forbid Rust `unsafe` code. Non-test builds deny
`clippy::unwrap_used`, `expect_used`, `panic`, `unreachable`, `todo`,
`unimplemented`, and `indexing_slicing`. Both crate roots enforce the same rules;
`cargo clippy --all-targets -- -D warnings` checks the production builds as well
as tests. Test assertions intentionally fail when a regression occurs.

Runtime changes include:

- Checked atom/bond/template lookups and slice iteration in editing, placement,
  selection handles, previews, rendering and the chemistry bridge.
- Fallible worker startup, protocol decoding and output serialization; request
  counter exhaustion returns an error and allows worker restart.
- Checked clipboard ID allocation. Invalid fragments, missing references or
  exhausted IDs leave the destination untouched.
- Validated edits roll back before entering history when their document is
  invalid. Template attachment rejects invalid geometry and source graphs.
- Safe signed-charge formatting and saturating charge changes, including
  `i32::MIN` and `i32::MAX` inputs.
- UTF-8-safe color/text/number parsing; bounded sequences and checked numeric
  increments. Missing or malformed bundled templates produce a library notice;
  invalid bundled style JSON falls back to explicit JACS settings.
- SVG string construction no longer unwraps formatting results. Image exports
  validate their document, retain the existing raster size cap, and return
  allocation/encoding errors.

Regression coverage exercises dangling bonds, stale anchors and handles,
invalid geometry, exhausted IDs, extreme charges, Unicode and invalid text
ranges, malformed chemistry requests, and recovery on the following valid
request. It also checks atomic rollback and the bundled JACS/catalog data.

Validation: 125 Rust tests and 36 Python tests passed; formatting and strict
Clippy passed. The current application was exercised through native computer
control for label editing, dragging, Undo/Redo and save/open. Logs are under
`artifacts/atom-labels-qa-20260920/` (local, ignored by Git).

These checks cover Moruno's runtime source and the exercised inputs. They do
not prove that operating-system services or third-party libraries can never
fail. The chemistry worker remains a separate process, and bridge errors are
reported without replacing the current drawing.

## Native clipboard update

The macOS helper uses checked optionals and explicit errors, without forced unwraps. Requests/responses are bounded, child execution times out after ten seconds, and the Rust process concurrently drains stdout/stderr. The helper validates all representations before clearing the clipboard. Cut deletes only after a successful write; captured document revisions prevent delayed Cut/Paste from changing newer work.

Binary drawing parsing bounds input to 16 MB, nesting to 64 levels, objects to 100,000 and properties to one million. Malformed lengths, duplicate nonzero identifiers, unsupported objects/properties and detected query predicates produce recoverable errors. Raster clipboard bounds are finite and checked before fixed-point encoding. See [clipboard verification and limits](clipboard.md).

This update passes **135 Rust tests**, **42 Python tests**, formatting and strict Clippy. Test totals above record earlier milestones.
