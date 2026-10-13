# Local chemical naming

Available in Nightly builds; not in ReShiki 0.11.0.

Open **Import → Name** to turn a supported chemical name into an editable
structure. Right-click a molecule and choose **Show chemical name** to place a
verified systematic name below it. Right-click a molecule and choose
**Chemical names…** to work in both directions in the
[Chemical names inspector](#chemical-names-inspector). Both directions use local
deterministic Rust rules. Naming requires no Java, Python, naming service or
network connection.

## Parse a chemical name

1. Open **Import**, select **Name**, and enter a name such as `ethanol`.
2. Choose **Insert**. ReShiki parses the name locally, verifies the editable
   graph, and inserts it. **Add name below** is selected initially; turn it off
   to insert only the structure. The molecule and its caption form one Undo
   step. Undo removes both; Redo restores both.

**Preview** is optional. Open it to adjust the layout by dragging atoms, or
edit its SMILES and choose **Apply** to change the chemistry. **Reset** restores
the parsed graph. Preview and parser **Details** are collapsed initially.
**Cancel** stops a pending name import. Editing the name or switching back to
**Structure** cancels the pending import as well.

Changing the preview's chemistry can make the original input name inaccurate.
ReShiki identifies this change and omits the original name when inserting the
edited structure. Unapplied SMILES edits cannot be inserted.

The [earlier desktop review](changes/rust-chemical-naming.md) shows an older
design of the **Chemical names** inspector, not the Import dock.

The Rust OPSIN port handles supported systematic and retained names, including
substituted functional groups, bicyclic/spiro systems, heterocycles, supported
absolute R/S and E/Z stereo, isotopes and formal charge. The editor accepts a
connected graph with at most 512 atoms. Ambiguous names, ignored or unsupported
stereo, relative/racemic qualifiers, radicals, polymers, queries and
disconnected salts reject with an explanation. Optical rotation signs are not
converted into R/S descriptors. The specific retained name `(+)-lactic acid`
is normalized locally to `(2S)-2-hydroxypropanoic acid`, using
[ChEBI:422](https://www.ebi.ac.uk/chebi/CHEBI:422), then parsed by the Rust
backend and checked against its explicit S graph. The parser result identifies
this retained-name source. Other optical-rotation names, including derivatives,
salts and conflicting descriptors, remain subject to strict rejection; this
entry is not a general sign-to-configuration rule. The original input can be
used for **Add name below**, while structure-to-name returns the verified
`(2S)-2-hydroxypropanoic acid`. Earlier native screenshots of this name rejecting
remain evidence of the older build; they do not validate this change.

## Generate a name from a structure

1. Right-click a molecule and choose **Show chemical name**. The clicked
   connected molecule is the target even when other molecules are selected or
   grouped with it. Abbreviated groups are expanded for naming.
2. ReShiki generates and verifies its systematic name locally. For ethanol the
   result is `ethan-1-ol`, centered below the molecule's visible drawing.
3. Choose **Hide chemical name** from the same menu to remove it. Show and Hide
   each form one Undo step. Showing an already named molecule reuses its caption.

To see or copy a name without placing it first, select one complete molecule
and choose **Generate selected name** in **Import → Name → Details** or in the
[Chemical names inspector](#chemical-names-inspector). Then choose **Copy name**
or **Insert caption**.

The caption is ordinary editable text with an optional native association to
its complete molecule. It follows layout changes. A chemical identity change
removes an obsolete automatic caption in the same Undo step; Undo restores it.
Editing the caption text or dragging it independently detaches it, preserving
that custom text through later chemical edits. Atom maps and visual atom
numbers do not change name identity.

Native save, reopen and whole-molecule copy retain the association. Copying a
caption alone retains plain text. Figure export and CDXML/CDX retain visible
caption text; external formats and older ReShiki readers cannot retain the
association. The optional metadata uses the existing document-version
convention, so an older reader may save the caption as ordinary text.

The **ReShiki organic rules 1** profile covers supported neutral organic
structures with at most 64 heavy atoms. Its declared subset includes carbon
chains with supported branching and unsaturation, simple cycloalkanes, benzene
and selected heteroaromatic parents; acids, simple esters, primary amides,
nitriles, aldehydes, ketones, alcohols, primary amines, ethers and halogen
substituents; and supported parent R/S and E/Z configurations. Carbon stems are
limited to 20 atoms and substituent nesting to three levels.

Charged or isotope-labelled graphs, fused/spiro/bicyclic parents, unsupported
functional groups and stereo contexts return an error. A generated name appears
only after the local Rust parser reconstructs the same graph and specified
stereo. Parent selection and naming rules also have independently expected
tests. This is a systematic-name subset; comprehensive IUPAC coverage and
preferred IUPAC name (PIN) selection are not claimed. Unspecified stereo remains
unspecified.

Changing the input or drawing invalidates pending or outdated results. An
unsupported or ambiguous name leaves the drawing and Undo history untouched.

See the [native review and fixtures](changes/rust-chemical-naming.md) for the
tested earlier build, examples, provenance and reproducible checks. The
[compact dock and molecule-caption follow-up](changes/molecule-name-label.md)
records the subsequent implementation and its verification status.

## Chemical names inspector

Right-click a molecule and choose **Chemical names…**. ReShiki selects that
molecule and opens the **Chemical names** inspector. It has two parts:

- **NAME → STRUCTURE**: enter a name and choose **Parse name**. Drag atoms in
  the preview to arrange the layout, or edit its SMILES and choose **Update**;
  **Restore** returns to the parsed structure. If the result has warnings,
  check **I reviewed these warnings and the structure**. Choose **Insert** to
  add the editable structure. It is inserted without a name caption.
- **STRUCTURE → NAME**: choose **Generate selected name**, then **Copy name** or
  **Insert caption**. Curated aliases for the molecule appear below the
  generated name.

**‹ Properties** returns to the Properties inspector, and **Import** opens the
Import tab.

## Curated offline aliases

Open **Import → Name → Details** to resolve the entered name and see its
**CURATED ALIASES · OFFLINE**. Opening Details does not open the optional
preview or insert anything. Choose **Copy** for an alias, or **Use below** to
use that alias for the next insertion's linked caption. This explicit choice
leaves the entered name unchanged; **Use original name below** restores the
original input as the caption. Changing the input clears the alias choice.
If Details remains open after editing, choose **Resolve name** to request the
new aliases without opening Preview or inserting. Resolve is disabled while
parsing and for empty input. Changing preview chemistry prevents using aliases
from the original graph.

To name a drawing, select one complete molecule and choose **Generate selected
name** in the same Details area. The **GENERATED SYSTEMATIC NAME · LOCAL RULES**
result and its **Copy name** / **Insert caption** actions remain separate from
the curated aliases and their **Copy** / **Insert caption** actions. Inserting
an alias reuses the molecule's linked caption instead of making a duplicate.
The action is one Undo step and retains the existing save/reopen, export and
caption-detachment behavior described above. After chemistry changes, generate
a new result before using it.

The bundled catalog currently contains 40 retained or functional-class names
for 30 identities, including `ethyl alcohol` for ethanol. Each alias displays
its source, version, record and license. The [complete catalog and provenance](../src/naming/catalog/README.md)
record the selected OPSIN MIT data and one ChEBI CC BY 4.0 equivalence.
The S, R and unspecified lactic acid graphs have separate entries. Neither
charge nor isotope variants inherit the neutral, unlabelled alias. Reaction
map labels do not change chemical identity. An uncatalogued graph explicitly
shows **No curated aliases available in the bundled catalog.** This means the
finite catalog has no match, not that the molecule has no other names.

The aliases come from a finite catalog bundled with ReShiki. Looking them up
needs no network connection, Java, Python or other program. The
[alias review](changes/offline-name-synonyms.md) records screenshots, saved-file
checks and limits.
