# Template library: original checks (2026-09-20)

These checks describe the first template library build and its connection
modes. The [template library page](../template-library.md) describes how the
library works today.

## Desktop check

ReShiki desktop checks used an isolated library directory:

- Searched for Pyridine, chose source bond 5–6 in its preview, and attached it
  to a drawn double bond. Check returned `C5H5N`, six atoms and six bonds; the
  original two endpoint IDs were retained.
- Added a caption, grouped it with the molecule, and saved **Tagged pyridine**
  in **Catalysts**. The persisted template contained six atoms, one caption and
  one group.
- Remembered source bond 4–5, marked the template as a favorite and placed it
  twice using Keep placing. Undo/Redo of the last placement preserved complete
  instances. The saved drawing contained 18 atoms, 18 bonds, three captions and
  three groups; Check returned `C15H15N3`.
- Exported the collection through the native Save dialog, removed its custom
  entry, and imported it again through the native Open dialog. The exported
  JSON matched the saved library, including the anchor and favorite.
- Restarted the standalone QA app outside the checkout. Favorites still
  contained the template, and its selected bond anchor reappeared in the larger
  preview.
- Renamed it to **Pyridine ligand**, moved it to **Ligands**, replaced it with
  the three-group drawing, then undid that library change. The six-atom
  template and its remembered anchor were restored; the drawing stayed
  unchanged.
- Inspected both the invalid-target message and the tinted free-placement
  preview in the final desktop build. Clicking the green preview committed
  black JACS drawing colors; Undo restored the saved three-instance drawing.
  Selecting a grouped caption also kept Templates open. Local evidence was kept
  under the ignored `artifacts/template-library-qa-20260920/` folder.

## Connection modes

Desktop checks joined the chosen furan carbon to benzene: 11 atoms, 12 bonds,
formula `C10H8O`. Selecting a furan bond changed the mode to fusion; dragging
from a benzene single bond produced 9 atoms, 10 bonds and `C8H6O`. Structure
checking accepted both products. Automated tests distinguish 2- and
3-phenylfuran, exercise every eligible furan/benzene edge against all six
benzene edges, preserve existing coordinates, and reject a saturated oxygen
attachment. Cleanup preview, original comparison, Cancel, Apply and Undo were
also exercised on the connected structure.

## Tests

That build passed **89 Rust tests**, including exact-anchor regioisomer
identity, wrong-anchor rejection, mixed-object transformation and group
remapping, hit testing, authoring isolation from drawing history, repeated
placement and Undo, persistence, conflict-safe merging, stale concurrent writers
and corruption handling. The existing **23 Python tests** covered the worker at
that milestone. Formatting, Clippy and bundle verification accompanied that
build.
