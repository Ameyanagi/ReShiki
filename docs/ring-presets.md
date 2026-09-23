# Ring presets and expanded template catalog

The later [Haworth update](haworth-projections.md) adds five/six-member perspective outlines and 12 carbohydrate templates, bringing the current library to 93 entries. The original 81-entry milestone is described below.

JACS / ACS remains the default. The new chair and cyclopentadiene tools use 14.4 pt edges (42 world units), Arial 10 pt labels and 0.6 pt lines unless the user changes the preferred bond length.

## Ring workflow

The main palette has separate **Chair A**, **Chair B** and **Cyclopentadiene** tools. Their contextual picker switches between these and the existing regular ring tool. Properties shows a fitted molecular preview. The canvas previews transient bonds in green; committed bonds use black JACS styling.

- Click empty space to place a ring centered at the pointer. Drag from that center to rotate it before committing.
- Click a compatible atom to share it, or a single/double bond to fuse. Drag from the target to choose the attachment side. Existing atoms and coordinates remain unchanged; new edges match the local bond length.
- Hold Alt/Option on an atom to attach the whole ring through a new single bond. The default direction uses the available angular gap; a drag chooses the direction. This mode also supports eligible nitrogen/oxygen attachment atoms. It does not replace an element with carbon.
- Hold Shift with Cyclopentadiene to move its two nonadjacent double bonds by one edge.
- Each placement is one Undo step. Escape, focus loss or release outside the canvas cancels a drag. Invalid chemistry leaves the document unchanged and explains the failed attachment.

Chair geometry uses three unit edge vectors and their opposites, giving six equal-length edges and two inward corners. A/B are mirrored **2D drawing projections**. They do not assign stereochemistry or encode a conformational transition. The stored result is a normal molecular graph, so labels, bonds, selection, grouping, native save and molecular/drawing exports remain editable. Cleanup may redraw the ring as a regular polygon.

The existing regular 3–8 member tool retains its previous interaction. Alt connection and freely rotated placement described here apply to the three new presets. General aromatic fusion, attachment at stereocenters/mapped or explicit-H sites, axial/equatorial substitution controls and 3D conformer operations remain incomplete.

## Catalog

There are 79 chemical catalog entries plus the two generated chair projections, for **81 built-ins**. Collections cover the original rings/small molecules, additional cycloalkanes and unsaturated rings, common heterocycles, bicyclic/cage structures, all 20 standard amino acids and five nucleobases.

Names, formulas, standard InChIKeys, SMILES and source URLs are frozen in [template-catalog.json](../assets/template-catalog.json). These chemical facts were retrieved from the [PubChem PUG REST service](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest). The amino-acid generator uses RDKit's documented [L-protein sequence flavor](https://www.rdkit.org/docs/source/rdkit.Chem.rdmolfiles.html). Its formula and standard InChIKey must agree with the corresponding PubChem record before generation succeeds. Glycine is achiral; the other amino-acid entries retain L stereochemistry. Entries are neutral free amino acids, not connected peptide residues.

`uv run --locked python scripts/regenerate_templates.py` regenerates the stored molecular coordinates without network access. It validates every catalog entry first and only then replaces `assets/templates.json`. The chair toolbar and library use the same Rust geometry generator.

Search includes amino-acid three-letter codes and other stored keywords. Exact names/codes rank before prefix and substring matches, so `Phe` brings phenylalanine ahead of thiophene. Entry notes state meaningful distinctions such as L configuration and unspecified decalin ring-junction stereochemistry. Built-in IDs retain the original group/name identity, so existing favorites continue working; user-authored templates remain in the separate local collection.

The catalog does not yet provide sugar stereoisomer families, DNA/RNA assemblies, peptide synthesis/sequence assembly, semantic protecting-group nicknames, polymer templates or many specialized chemical families.

## Verification

A CDXML fixture in `tests/fixtures/` contains both chair orientations and a cyclopentadiene: 17 atoms, 17 bonds, three rings and formula C17H30. Its atom coordinates and molecular identity survive interchange.

Automated tests cover equal JACS edge lengths, mirrored/reflex chair geometry, free rotation, shared-atom/fused-bond attachment at nondefault scale, whole-ring connection through oxygen, full-valence rejection, shifted diene bonds, native/CDXML geometry, input gestures and Undo/Redo. Every catalog structure is checked through analyze/cleanup against its frozen PubChem formula and standard InChIKey; all 20 amino acids additionally retain identity through MOL exchange. The complete suite passes **111 Rust and 32 Python tests**.

ReShiki desktop verification created a chair fused onto a drawn bond, a freely rotated second chair, cyclopentadiene and an Option-dragged chair connected through oxygen. The saved drawing has 24 atoms, 24 bonds and formula C23H42O; all bond lengths measure 42 world units within 0.001. The oxygen remains oxygen. A separate library placement of L-phenylalanine preserves its formula and standard InChIKey. Evidence is in `artifacts/rings-qa-20260920/desktop-rings.reshiki`, `desktop-phenylalanine.reshiki` and `option-chair-preview.png`. The ring drawing's PNG and rendered PDF were visually inspected.

That desktop check exposed an extra Undo step from **Check structure** refreshing computed hydrogen labels. Check now updates its display cache and properties without rewriting the drawing's bond orders/stereochemistry, changing selection, clearing Redo or adding history. Computed hydrogen labels alone do not make a saved drawing dirty. Two regression tests cover saved-state behavior and placement/Check/Undo/Redo.

The final optimized standalone app was exercised again: place chair → Check → one Cmd+Z leaves an empty drawing; Cmd+Shift+Z restores C6H12. `Phe` now shows L-phenylalanine first. Changing the current text defaults to 18 pt bold red, then creating a new document, restores Arial 10 pt black and JACS/ACS. Screenshots: `jacs-default.png` and `phe-search.png` in the same artifact folder. The signed bundle passes its engine check outside the checkout.
