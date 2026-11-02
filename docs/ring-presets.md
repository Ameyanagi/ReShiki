# Ring presets and expanded template catalog

The current library contains 122 built-in templates, including the [Haworth carbohydrates](haworth-projections.md), macrocycles, free-base porphine and ligands. The original ring-tool milestone is described below.

JACS / ACS remains the default. The new chair and cyclopentadiene tools use 14.4 pt edges (42 world units), Arial 10 pt labels and 0.6 pt lines unless the user changes the preferred bond length.

## Ring workflow

In 0.9.0, **Benzene**, six-membered **Aromatic circle**, and aromatic templates share validated fusion. An aromatic-carbon click attaches a phenyl group by a new single bond. Eligible edge gestures reuse shared vertices, including supported inward bays and gaps whose atoms already exist. Mixed circular and Kekulé drawing styles remain usable for subsequent fusion. Invalid valence and duplicate overlays leave the drawing unchanged; nonaromatic closures keep their valid bond pattern. [Release details](changes-0.9.md#aromatic-fusion-and-phenyl-attachment).

The main palette has separate **Chair A**, **Chair B** and **Cyclopentadiene** tools. Their contextual picker switches between these and the existing regular ring tool. Properties shows a fitted molecular preview. The canvas previews transient bonds in green; committed bonds use black JACS styling.

- Click empty space to place a ring centered at the pointer. Drag on empty space instead to place the ring's first atom at the press point with its first bond pointing at the pointer, the ring on the counter-clockwise side. The direction snaps to 15° steps; hold Option/Alt to rotate freely. The ring keeps its usual bond length either way; dragging never scales it.
- Click a compatible atom to share it, or a single/double bond to fuse. Drag from the target to choose the attachment side; a drag from an atom aims the ring in 15° steps, or freely with Option/Alt. Existing atoms and coordinates remain unchanged; new edges match the local bond length.
- Hold Alt/Option on an atom to attach the whole ring through a new single bond. The default direction uses the available angular gap; a drag chooses the direction freely. This mode also supports eligible nitrogen/oxygen attachment atoms. It does not replace an element with carbon.
- Hold Shift with Cyclopentadiene to move its two nonadjacent double bonds by one edge.
- Each placement is one Undo step. Escape, focus loss or release outside the canvas cancels a drag. Invalid chemistry leaves the document unchanged and explains the failed attachment.

Chair geometry uses three unit edge vectors and their opposites, giving six equal-length edges and two inward corners. A/B are mirrored **2D drawing projections**. They do not assign stereochemistry or encode a conformational transition. The stored result is a normal molecular graph, so labels, bonds, selection, grouping, native save and molecular/drawing exports remain editable. Cleanup may redraw the ring as a regular polygon.

The regular 3–8 member tool rotates the same way when dragged on empty space or from an atom; a drag from empty space released on an atom or bond still attaches there. Alt connection applies to the presets. General aromatic fusion, attachment at stereocenters/mapped or explicit-H sites, axial/equatorial substitution controls and 3D conformer operations remain incomplete.

## Catalog

There are 108 chemical catalog entries, two generated chair projections and 12 Haworth entries, for **122 built-ins**. Collections cover rings/small molecules, cycloalkanes and unsaturated rings, common heterocycles, bicyclic/cage structures, all 20 standard amino acids, five nucleobases, macrocycles and ligands. Retained fused parents include anthracene, phenanthrene, azulene, indene, fluorene, pyrene, carbazole, acridine, quinazoline, quinoxaline, benzimidazole, indazole, chromene, dibenzofuran and dibenzothiophene.

Names, formulas, standard InChIKeys, SMILES and source URLs are frozen in [template-catalog.json](../assets/template-catalog.json). These chemical facts were retrieved from the [PubChem PUG REST service](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest). The amino-acid generator uses RDKit's documented [L-protein sequence flavor](https://www.rdkit.org/docs/source/rdkit.Chem.rdmolfiles.html). Its formula and standard InChIKey must agree with the corresponding PubChem record before generation succeeds. Glycine is achiral; the other amino-acid entries retain L stereochemistry. Entries are neutral free amino acids, not connected peptide residues.

`uv run --locked python scripts/regenerate_templates.py` updates the stored templates without network access. It validates catalog formulas and standard InChIKeys, reuses unchanged stored documents and generates coordinates for changed or new structures before replacing `assets/templates.json`. Per-entry drawing SMILES select the 9H tautomers of purine, adenine and guanine while retaining their frozen PubChem facts; 7H-Purine remains available separately. Orientation metadata places the new fused parents in the IUPAC FR-5.2 orientation. The chair toolbar and library use the same Rust geometry generator.

Search includes IUPAC names, von Baeyer names, amino-acid codes and biochemical symbols such as `Ade`, `Glcp` and `a-D-Glcp` / `α-D-Glcp`. Superscript digits and caret/braces spellings fold to plain digits. Exact names/codes rank before prefix and substring matches, so `Phe` brings phenylalanine ahead of thiophene. Entry notes state meaningful distinctions such as L configuration and unspecified decalin ring-junction stereochemistry. Built-in IDs retain the original group/name identity, including renamed 9H entries, so existing favorites continue working; user-authored templates remain in the separate local collection.

Ring templates, including crown ethers, cyclen and cyclam (not porphine), carry static atom locants for the large template source preview and attachment labels such as **Attach at N-9**. Substituents are unnumbered. Locants stay out of the drawing, thumbnails, join panel and exports. Acridine uses N-10 and decalin uses naphthalene-style 4a/8a fusion locants.

The catalog does not yet provide sugar stereoisomer families, DNA/RNA assemblies, peptide synthesis/sequence assembly, semantic protecting-group nicknames, polymer templates or many specialized chemical families.

## Verification

A CDXML fixture in `tests/fixtures/` contains both chair orientations and a cyclopentadiene: 17 atoms, 17 bonds, three rings and formula C17H30. Its atom coordinates and molecular identity survive interchange.

Automated tests cover equal JACS edge lengths, mirrored/reflex chair geometry, free rotation, shared-atom/fused-bond attachment at nondefault scale, whole-ring connection through oxygen, full-valence rejection, shifted diene bonds, native/CDXML geometry, input gestures and Undo/Redo. Every catalog structure is checked through analyze/cleanup against its frozen PubChem formula and standard InChIKey; all 20 amino acids additionally retain identity through MOL exchange. The complete suite passes **111 Rust and 32 Python tests**.

ReShiki desktop verification created a chair fused onto a drawn bond, a freely rotated second chair, cyclopentadiene and an Option-dragged chair connected through oxygen. The saved drawing has 24 atoms, 24 bonds and formula C23H42O; all bond lengths measure 42 world units within 0.001. The oxygen remains oxygen. A separate library placement of L-phenylalanine preserves its formula and standard InChIKey. Evidence is in `artifacts/rings-qa-20260920/desktop-rings.reshiki`, `desktop-phenylalanine.reshiki` and `option-chair-preview.png`. The ring drawing's PNG and rendered PDF were visually inspected.

That desktop check exposed an extra Undo step from **Check structure** refreshing computed hydrogen labels. Check now updates its display cache and properties without rewriting the drawing's bond orders/stereochemistry, changing selection, clearing Redo or adding history. Computed hydrogen labels alone do not make a saved drawing dirty. Two regression tests cover saved-state behavior and placement/Check/Undo/Redo.

The final optimized standalone app was exercised again: place chair → Check → one Cmd+Z leaves an empty drawing; Cmd+Shift+Z restores C6H12. `Phe` now shows L-phenylalanine first. Changing the current text defaults to 18 pt bold red, then creating a new document, restores Arial 10 pt black and JACS/ACS. Screenshots: `jacs-default.png` and `phe-search.png` in the same artifact folder. The signed bundle passes its engine check outside the checkout.
