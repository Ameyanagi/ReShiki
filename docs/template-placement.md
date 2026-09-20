# Template previews and attachment

Historical first implementation; see [current connection modes](template-library.md) for the expanded behavior.

2026-09-20. The Templates inspector now shows molecular thumbnails for all twelve built-in structures. Choosing a card enters placement mode without adding anything. Moving over the canvas previews the whole fragment. Click empty space to insert, an atom to share a compatible vertex, or a bond to share its endpoints and bond. Drag from an attachment to choose the side.

Placement returns to Select, with handles around the inserted fragment. This prevents an extra preview from obscuring the completed structure. Escape cancels placement, and focus loss cancels an active drag. Completed placement is one Undo step.

Geometry matches the existing bond length and orientation. Candidate attachment atoms must match element, charge and isotope, with compatible bond orders and available valence. Incompatible targets show a red marker and clicking them reports the reason without changing the document. The subsequent [template library update](template-library.md) adds exact source-anchor selection, custom mixed-object templates, search/favorites, collection import/export and repeat placement. General aromatic re-kekulization remains incomplete.

The subsequent typography build also permits saturated ring templates on plain double bonds when valence allows it. The existing double bond is retained: cyclopentane and cyclohexane templates then produce cyclopentene and cyclohexene. Unsaturated templates still require matching bond orders, and triple/stereo/aromatic target bonds remain restricted. Automated geometry and chemical-identity tests cover these additional placements at three orientations.

Verified this addition through computer use in the final build: drew a double bond, selected Cyclopentane, dragged upward from the bond midpoint, then checked the result. The inspector reported 5 atoms, 5 bonds, `C5H8`, canonical `C1=CCCC1`. Saved the native drawing and screenshot under `artifacts/style-qa-20260920/`.

The source structures in `assets/templates.json` were generated from Moruno's existing SMILES library with its RDKit worker. Thumbnails use the same scene renderer and structure data as placement. Regenerate coordinates with `uv run --locked python scripts/regenerate_templates.py`.

## Verification

- **Desktop:** launched a separate QA app and recovery directory. Drew a two-carbon single bond. Selected Pyridine, observed its attached preview before insertion, then dragged upward from the bond. The resulting molecule reused the original two atoms; Check showed 6 atoms, 6 bonds, `C5H5N`, canonical `c1ccncc1`. Saved through the native dialog as a local test artifact.
- **Desktop after final UI adjustment:** repeated placement and confirmed that it returned to Select with resize/rotation handles and no extra template preview. A single Undo restored the original bond; Redo restored pyridine.
- **Automated:** all twelve library structures retain canonical identity on placement. Bond fusion tests cover three orientations at a different bond length for cyclopentane, pyridine, furan and benzene. Additional checks cover methylcyclohexane from atom attachment, naphthalene from aromatic double-bond fusion, side choice, incompatible targets, cancellation, unchanged document on template choice, and exact Undo/Redo.
- **Checks:** 45 Rust tests and 10 Python tests passed; formatting, Clippy, build, bundle signature verification and the standalone bundled chemistry check passed.

Local screenshots and fixtures are in `artifacts/` (ignored by Git). The development and standalone app binaries were updated. Existing user document windows were not restarted.
