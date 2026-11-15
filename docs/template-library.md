# Template library and exact attachment

The bundled groups and templates are the current implementation. Their coverage, labels and layouts may be extended or revised in future releases. Existing examples can be saved as editable native drawings.

See also [template placement](template-placement.md). Templates retain explicitly saved formatting and geometry.

## Search

Search matches names, collections, SMILES and keywords. Exact names and keyword
codes rank ahead of prefix and substring matches. IUPAC aliases such as
`1,3-oxazole`, `oxolane` and von Baeyer names are searchable, and superscript
digits, plain digits and caret or brace spellings are equivalent. Biochemical
symbols include `Ade`, `Glcp` and anomer forms such as `α-D-Glcp` or
`a-D-Glcp`. Informal nucleoside aliases such as `A` and `dT` rank below exact
names and standard symbols such as `Ado` and `dThd`. Built-in heteroaromatic
rings belong to both **Aromatics** and **Heterocycles**. Back and Forward
restore the filters, scroll position and chosen source anchor.

## Placement

The chosen source anchor is never silently replaced. Existing atom positions and
identities stay fixed, and the live preview uses the same constrained direction
as the final placement. A new-bond connection aligns the incoming atom's open
valence with the joining bond: a phenyl vertex has 120° angles to both ring
edges even on a rotated five-membered ring. Bundled ring templates show locants
such as **Attach at C-3** in the source preview only; they never reach the
canvas, thumbnails, join panel or exports.

For common neutral five- and six-membered aromatic rings, fusion can move single
and double bonds across the combined ring system, so the chosen edge decides the
product whatever the starting Kekulé phase. This is a bounded aromatic subset,
not a general aromaticity or reaction engine. Other incompatible elements, bond
styles, stereocenters or valences return a recoverable error before release.

In empty space, the source anchor lands at the pointer, or the template center
with **Auto**, and a drag rotates around that point. Captions stay upright while
their positions, arrows and graphics move with the template. Attachment matches
the target bond length; text sizes and stroke widths keep their saved values.
Placement commits the saved colors, and each placement is one Undo step.

Coordination geometry frames (SP-4, TBPY-5 and OC-6) follow the
[IUPAC drawing recommendations](https://iupac.qmul.ac.uk/drawing/stereo.html).
Sharing a frame's metal with a drawn metal keeps the drawn element, charge and
identity. The frames are depictions only: their wedges show projection and
assign no tetrahedral R/S or non-tetrahedral stereochemistry. SVG keeps the
wedges; CDXML and CDX keep them without assigning tetrahedral stereo at the
metal.

## Custom templates

Saving a selection captures its atoms, captions, arrows and graphics, and keeps
nested and integral groups whose members are all included. Saving writes only
the library, never the drawing or its history. **Replace from selection**
resets the remembered anchor to Auto because object IDs may change. **Remove
template** removes only the library entry, and **Undo library change** restores
the latest replacement, anchor change or removal until the next library write.
Placed instances are independent of the library. Built-in entries cannot be
edited; place one and save its selection to make an editable copy.

## Persistence and exchange

Custom entries and favorite IDs are stored in `templates.json` in ReShiki's local application data directory, or `RESHIKI_DATA_DIR` when specified. Writes use a complete temporary file and atomic replacement. OS file locking serializes local library writes; a stale editor must Reload before writing. Invalid/corrupt collections are reported and never silently overwritten.

**Export** writes custom entries and favorites as a versioned `.reshiki-templates` JSON collection. **Import** validates the whole collection before merging it. Identical templates are deduplicated, repeated imports are idempotent, and conflicting edited versions are kept as separate entries with distinct IDs. Import/export does not modify the current drawing. Collections use ReShiki's own file format.

## Biological templates

**Nucleosides** contains Ado, Guo, Cyd, Urd, Thd, dAdo, dGuo, dCyd and dThd, plus AMP, GMP, CMP, UMP, dAMP and dTMP. Each uses a β-D-ribofuranose or 2-deoxy-β-D-ribofuranose Haworth sugar, with the base attached at N9 for purines or N1 for pyrimidines. The base lies above C1′; 5′-monophosphates show explicit O5′–P(=O)(OH)₂ atoms extending leftward from C5′. The thymine pair is named **5-Methyluridine (Thd)** and **2′-Deoxythymidine (dThd)**, so the historical Thd/thymidine clash cannot mislead; `thymidine` is also a search keyword for dThd. Frozen independent [PubChem facts](../assets/nucleosides.json) gate every generated document’s formula and standard InChIKey.

**Steroids** contains gonane, estrane, 5α-androstane, 5α-pregnane, 5α-cholestane, cholesterol, testosterone and estradiol. The regular ring geometry follows IUPAC orientation 2a, with A at lower left and D at upper right. Bold β and hashed α bonds end at real, labelled junction H atoms; the C-18 and C-19 methyls are bold skeletal bonds. Each structure also offers **implicit 8,9,14-H**, retaining the same stored configuration while omitting those three H atoms and their bonds. Gonane and estrane leave C5 unspecified, matching their independent [PubChem records](../assets/steroids.json); no C5–H bond is drawn for those parents. Native files and MOL/SMILES keep the stored 8/9/14 configurations. Editable CDXML/CDX writes the drawing as shown, so ChemDraw reads those three centres as unspecified; use the full form for ChemDraw exchange.

## Remaining limits

- The library now contains 202 built-ins. Common rings and retained fused parents, two chair projections, 21 neutral free amino acids (including L-selenocysteine, `Sec`/`U`), five nucleobases, 50 [Haworth carbohydrate entries](haworth-projections.md), seven [Fischer projections](haworth-projections.md#fischer-projections), 15 nucleosides/nucleotides, 16 steroid drawings, macrocycles (including [Porphine (21H,23H)](porphine-core.md); search `porphyrin`), ligands and three coordination frames are included. Adenine, guanine and the main purine entry use the 9H tautomer for N-9 attachment; a separate 7H-Purine retains the PubChem tautomer. Their original favorite IDs are preserved. DNA/RNA chain assembly, peptide construction and many specialized families remain missing. See [catalog and ring presets](ring-presets.md).
- Ordinary attachment requires matching supported elements, charge/isotope and available valence; it excludes stereocenters, explicit-H/mapped sites and unsupported target bond styles. Metal-to-metal atom sharing also accepts differing elements and charges while retaining the drawing atom's identity. General aromatic re-kekulization and arbitrary post-insertion joining remain incomplete.
- There is no modifier-driven free resizing during placement or viewport auto-pan. Selection handles can resize a placed instance.
- Captions remain upright under rotation, and the existing annotation model does not support arbitrary rotated text.
- Collections contain at most 2048 custom templates, each with at most 10000 drawing objects; imports are limited to 32 MB. There is no shared cloud library or third-party palette/CDX interchange.

The original 2026-09-20 desktop and test checks are in
[the template library record](changes/template-library-20260920.md). The
[feature status](feature-status.md) page tracks the remaining gaps.
