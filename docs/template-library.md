# Template library and exact attachment

Added 2026-09-20 after the initial [template placement](template-placement.md) update. JACS/ACS remains the default drawing style. Templates retain explicitly saved formatting and geometry.

## Find and place

The Templates inspector has name/collection/SMILES/keyword search, compact category rows such as Aromatics, **All / My templates / Favorites**, and a four-column grid of small structure icons inside each category. Names appear on hover instead of taking space under every icon. Exact names and keyword codes rank ahead of prefix/substring matches. Choosing an icon opens a larger source preview and enters placement mode without changing the drawing. **Browse templates** returns to the results. Mouse Back/Forward and Alt+Left/Right navigate between the detail preview, category and category list, restoring filters, scroll position and the chosen source anchor.

Click an atom or bond in the source preview to highlight that exact attachment point. Choose **Connect with a bond** to join two atoms with a new single bond, **Share an atom** to merge compatible atoms, or **Fuse along a bond** to share an edge and its endpoints. Clicking a source bond selects fusion; clicking an atom after fusion returns to connection mode. **Auto** chooses a compatible source within the selected mode. Empty space always places a separate template.

The chosen source is never silently replaced. Drag from the target to choose the direction or side. Hold Shift or Ctrl to snap to the same 15° increments as normal bond drawing; Alt temporarily frees the angle. The live preview and final placement use the same constrained direction. Existing atom positions and identities remain fixed. New-bond connections align the incoming atom’s open valence exactly with the joining bond: a phenyl vertex has 120° angles to both ring edges even when the target five-membered ring is rotated. For common neutral five- and six-membered aromatic rings, fusion can redistribute single/double bonds across the combined ring system. This lets the chosen edge determine the product independently of the initial Kekulé phase. It is a bounded aromatic subset, not a general aromaticity or reaction engine. Other incompatible elements, bond styles, stereocenters or valences return a recoverable error.

In empty space, the chosen source anchor lands at the pointer; Auto centers the template. A drag rotates the template around that placement point. Captions stay upright while their positions, arrows and graphic geometry move with the template. Attachment matches the target bond length; text sizes and stroke widths retain their saved publication values.

The live molecular/graphic preview is tinted green and a preview strip distinguishes it from committed objects. An invalid target reports its reason before release. Placement commits the saved colors, including black JACS defaults. Each placement is one Undo step. **Keep placing** enables repeated insertion; Escape returns to Select. A drag also cancels on focus loss or release outside the canvas.

## Author and maintain

1. Select the desired atoms, captions, arrows and/or graphics, then choose **Save selection as template**. Existing nested/integral groups are retained when their members are included. The form captures this selection immediately.
2. Enter a name and collection, then Save. This writes the library without editing the drawing or its history.
3. Choose an attachment point in the source preview and **Remember anchor** to make it the custom template's default. Favorites and remembered anchors survive restarting ReShiki.
4. **Edit** changes the name or collection. To revise the drawing, edit an instance on the canvas, select the revised objects, and choose **Replace from selection**. Replacement resets the source anchor to Auto because object IDs may have changed.
5. **Remove template** removes only the library entry. **Undo library change** restores the most recent replacement, anchor change or removal until another library write occurs. Existing drawing instances remain independent.

Selecting a caption or graphic keeps the Templates inspector open, so authoring and replacement do not require switching back from Properties. Built-in entries are immutable; place one and save its selection to create an editable copy.

## Persistence and exchange

Custom entries and favorite IDs are stored in `templates.json` in ReShiki's local application data directory, or `RESHIKI_DATA_DIR` when specified. Writes use a complete temporary file and atomic replacement. OS file locking serializes local library writes; a stale editor must Reload before writing. Invalid/corrupt collections are reported and never silently overwritten.

**Export** writes custom entries and favorites as a versioned `.reshiki-templates` JSON collection. **Import** validates the whole collection before merging it. Identical templates are deduplicated, repeated imports are idempotent, and conflicting edited versions are kept as separate entries with distinct IDs. Import/export does not modify the current drawing. Collections use ReShiki's own file format.

## Verification

ReShiki desktop checks, using an isolated library directory:

- Searched for Pyridine, chose source bond 5–6 in its preview, and attached it to a drawn double bond. Check returned `C5H5N`, six atoms and six bonds; the original two endpoint IDs were retained.
- Added a caption, grouped it with the molecule, and saved **Tagged pyridine** in **Catalysts**. The persisted template contained six atoms, one caption and one group.
- Remembered source bond 4–5, marked the template as a favorite and placed it twice using Keep placing. Undo/Redo of the last placement preserved complete instances. The saved drawing contained 18 atoms, 18 bonds, three captions and three groups; Check returned `C15H15N3`.
- Exported the collection through the native Save dialog, removed its custom entry, and imported it again through the native Open dialog. The exported JSON matched the saved library, including the anchor and favorite.
- Restarted the standalone QA app outside the checkout. Favorites still contained the template, and its selected bond anchor reappeared in the larger preview.
- Renamed it to **Pyridine ligand**, moved it to **Ligands**, replaced it with the three-group drawing, then undid that library change. The six-atom template and its remembered anchor were restored; the drawing stayed unchanged.
- Inspected both the invalid-target message and the tinted free-placement preview in the final desktop build. Clicking the green preview committed black JACS drawing colors; Undo restored the saved three-instance drawing. Selecting a grouped caption also kept Templates open. Local evidence is under ignored `artifacts/template-library-qa-20260920/`.

Validation: **89 Rust tests**, including exact-anchor regioisomer identity, wrong-anchor rejection, mixed-object transformation/group remapping, hit testing, authoring isolation from drawing history, repeated placement/Undo, persistence, conflict-safe merging, stale concurrent writers, and corruption handling. The prior **23 Python tests** still cover the unchanged worker. Formatting, Clippy and bundle verification accompany the final build.

## Remaining limits

- The library now contains 81 built-ins. Common rings, two chair projections, 20 neutral free amino acids and five nucleobases are included; sugars, nucleotide/DNA/RNA assemblies, peptide construction and many specialized families remain missing. See [catalog and ring presets](ring-presets.md).
- Attachment still requires matching supported elements, charge/isotope and available valence; it excludes stereocenters, explicit-H/mapped sites and unsupported target bond styles. General aromatic re-kekulization and arbitrary post-insertion joining remain incomplete.
- There is no modifier-driven free resizing during placement or viewport auto-pan. Selection handles can resize a placed instance.
- Captions remain upright under rotation, and the existing annotation model does not support arbitrary rotated text.
- Collections contain at most 2048 custom templates, each with at most 10000 drawing objects; imports are limited to 32 MB. There is no shared cloud library or third-party palette/CDX interchange.

The [full feature-gap audit](feature-status.md) retains the broader outstanding scope.

## Connection-mode verification

Desktop checks joined the chosen furan carbon to benzene: 11 atoms, 12 bonds, formula `C10H8O`. Selecting a furan bond changed the mode to fusion; dragging from a benzene single bond produced 9 atoms, 10 bonds and `C8H6O`. Structure checking accepted both products. Automated tests distinguish 2- and 3-phenylfuran, exercise every eligible furan/benzene edge against all six benzene edges, preserve existing coordinates, and reject a saturated oxygen attachment. Cleanup preview, original comparison, Cancel, Apply and Undo were also exercised on the connected structure.
