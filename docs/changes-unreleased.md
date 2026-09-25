# Merged changes awaiting release

The approved stack, PRs #22–#31, was merged on 25 September 2026 after passing checks and visual review. The published release remains **0.7.1**. This page describes the development version and the additional shortcut-help work.

## Drawing and chemistry

- **Haworth projections:** carbohydrate templates with defined stereochemistry, plus five- and six-member outlines. Native, figure and checked editable exports retain supported depictions. Blank scaffolds do not infer stereochemistry. [Usage and tested scope](haworth-projections.md).
- **Contextual shortcuts:** element replacement, complete groups, atom growth, ring attachment and fusion, bond styles, arrangement and joining. Geometry fixes include linear triple bonds, ring attachment angles, branching and bold-bond junctions. [Complete shortcut list](contextual-shortcuts.md).
- **Explicit labels and composition:** NH3 keeps its three hydrogens at a donor contact; supported formula labels such as C2H5 and OCH3 create real groups. Enter can name a selected fragment. Label anchoring, reverse-label fallback and clipping were corrected. [Labels and abbreviations](abbreviations.md).
- **Independent resizing:** side handles change width or height; corner handles preserve proportions. Atom-owned circles and curves transform with the structure while fonts and stroke widths retain their sizes. [Selection transforms](selection-transforms.md).
- **Ring interior colors:** fill selected cycles, remove a fill, and retain ownership through copying, movement and resizing. Native, figure and supported editable exports preserve fills. [Visual examples](changes/pr-27.md).
- **Templates and groups:** 105 built-in templates, including macrocycles and ligands; MgBr and N3 shortcuts; Cp and arene multi-center attachments. [Library](template-library.md).
- **Crossings:** aromatic circles and tilted inner curves now honor front/back bond clearance. The gaps remain transparent in image copies. [Before and after](changes/pr-29.md).

![Ring interior colors and transformations](images/pr-reviews/pr27-ring-fills.png)

**Groups and templates may be modified or revised in future releases.** These definitions and layouts are the current implementation; users can use them now and keep editable copies or custom templates.

## Desktop, licensing and review records

- macOS opens native `.rsk` documents through Finder/Open With and preserves an existing edited document by using a separate window. [Native opening evidence](changes/pr-30.md).
- Original project code is offered under **MIT OR Apache-2.0**, copyright **2026 Ameyanagi and ReShiki contributors**. Third-party material retains its own terms and notices. The contribution agreement and CI check record the contributor's dual-license acceptance. [License scope](../LICENSE).
- PR and issue templates now request a feature image or matched bug-fix before/after images. The merged stack includes 35 reusable review images and editable fixtures. [All reviewed changes](changes/stack-22-30.md) · [Visual review guidance](visual-review.md).

## Shortcut help and direct drawing gestures

**Help → Open shortcut examples** opens one bundled, editable `.rsk` document in a separate window. Its nine pages contain labeled examples that can be selected and copied into another drawing. It works offline; Save as creates a personal copy. [Download and shortcut list](contextual-shortcuts.md).

Choose an element in Atoms and **drag from an existing atom** to add it with a single bond. Clicking still replaces an atom. Fixed length and angle settings apply; Option/Alt permits free placement. Connecting existing endpoints preserves their elements and existing bond order.

The Rings palette offers **Benzene** with alternating bonds and **Aromatic circle**. Hold **Cmd/Ctrl while placing a regular ring, benzene or cyclopentadiene** for the circle form. Select an existing aromatic ring and press **a** to toggle circle/alternating display. Chairs and Haworth tools keep their existing projection behavior.

[Native screenshots, all nine example pages, and before/after fixes](changes/shortcut-help.md).

![Help with the editable shortcut reference](images/shortcut-help/help.png)
