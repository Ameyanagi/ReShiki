# Command palette targeting on large selections

This change addresses [#587](https://github.com/Ameyanagi/ReShiki/issues/587).
Opening Commands after selecting a large drawing no longer rebuilds the same
atom and bond target for every result row and its detail. The palette computes
one target per view. Temporary ID sets make that calculation linear in the
document and selection sizes, while its returned vectors retain their original
order and duplicate entries.

There is no cross-frame target cache. Availability and command execution still
read the current drawing, selection, tool and keyboard hotspot. A selection
containing only captions, graphics or unknown IDs still prevents fallback to a
pointed atom. Hidden model atoms and collapsed abbreviation members keep their
existing selection semantics. No chemistry, document format or drawing output
changes are intended.

## Original failure

The baseline is `b956e46347f644372e3c01128b99ca64ac208e2d`. The unchanged
issue fixture is `tiled16-v26.rsk`, SHA-256
`71229e202a3a0ba5765af5ac4bd94f4396613237f421cb4c828925c589b553da`.
It contains 7,728 atom records and 6,864 bonds; Select All returns 9,856 object
IDs. With that selection, opening `:` kept the main thread busy and native
accessibility requests timed out. All 543 main-thread samples in the recorded
native sample were inside view construction's repeated target calculation.

The original query performs about 99.5 million ID comparisons for this one
target, before repeating it for each result row. This is arithmetic from the
source and fixture's actual Select All order, not a timing measurement. The
fix removes both the repeated calculation and its nested linear searches.

## Regression coverage

- A test-only copy of the original query checks exact atom and bond vectors and
  the pointed flag. Cases include selection order and duplicates, missing IDs
  and bond endpoints, duplicate records, captions and graphics, hidden models,
  collapsed groups, hover, keyboard atom/bond/blank targets, stale hover epochs,
  Select/Lasso and another drawing tool.
- An actual `App::view` regression counts target calculations: zero with the
  palette closed, one with it open across the many-row catalog, query changes,
  navigation, filtering and the selected detail. It also checks unchanged
  document, selection, camera, revision and history.
- Execution tests change the selection or file epoch after constructing the
  view, then verify that the command edits the current target in one Undo step.
  A formerly available command refuses a now-inapplicable selection.

Existing contextual ranking, non-atom selection precedence, stale template rows
and the 3D-scale recovery route remain in the focused palette suite. No new
dependency or global document index is introduced.

The focused optimized native test command passed all 34 palette tests:

```sh
cargo test --release --locked --offline -p reshiki --bin reshiki \
  app::command_palette::tests:: -- --test-threads=1
```

The locked, offline optimized native application build also passes. These checks
ran on Apple M1, macOS 27.0.1, with Rust 1.99.0 and two build jobs. The normal
source formatting and whitespace checks pass.

## Native usability check

The verified optimized application was opened with the exact issue fixture at
100% zoom, Properties visible and keyboard drawing off. After Cmd+A selected
all 9,856 IDs, `:` opened 852 command results and native accessibility requests
returned without the previous timeout. Typing `rotate` produced seven results;
Down selected the next result, and the Edit filter showed four results. Escape
closed the palette, reopening restored all 852 results, and executing Actual
size closed it. Undo and Redo stayed disabled, the document title stayed clean,
and no document edit or save was performed.

Root performed this bounded interaction check on the same Apple M1 setup used
for the optimized build. Another reference build could run concurrently, so
this establishes usability for the reported workflow rather than a precise
latency or isolated benchmark result. Screenshots and accessibility states were
observed during the check; no screenshot artifact was saved. The exact binary,
fixture and observations are recorded in `root-native-qa.json` under the
task-owned completion-audit evidence directory. Neither the comparison count
nor this interaction check establishes a numerical speedup.

Reusable caption: **Commands stay responsive on large selections while keeping
their existing selection and keyboard-target behavior.**
