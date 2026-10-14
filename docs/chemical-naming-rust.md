# Chemical names

Chemical names turns a name into an editable structure and gives a molecule its
systematic name. Names are converted on your computer; no internet connection,
Java or naming service is needed.

## Use it

### Insert a structure from a name

1. Open **Import**, choose **Name** and type a name such as `ethanol`.
2. Choose **Insert**. **Add name below** is on at first and adds the name as a
   caption below the structure; turn it off to insert only the structure. Undo
   removes both, and Redo restores both.
3. Optional: choose **Preview** to check the structure first. Drag atoms to
   arrange the layout, or edit the **SMILES** and choose **Apply**. **Reset**
   restores the parsed structure.

**Cancel** stops a pending import. Editing the name or switching back to
**Structure** cancels it too. If you change the chemistry in the preview, the
typed name no longer matches, so ReShiki inserts the structure without it.
Apply SMILES edits before you insert.

### Show a molecule's name

1. Right-click a molecule and choose **Show chemical name**. The name appears
   centered below the molecule; for ethanol it is `ethan-1-ol`.
2. Choose **Hide chemical name** in the same menu to remove it.

Show and Hide are each one Undo step. The clicked molecule is named even when
other molecules are selected or grouped with it. The caption moves with the
molecule. When the chemistry changes, the caption is removed in the same Undo
step. Editing or dragging the caption turns it into ordinary text that stays.

To see or copy a name without placing it, select one whole molecule and choose
**Generate selected name** in **Import → Name → Details** or in the
[Chemical names inspector](#chemical-names-inspector). Then choose **Copy name**
or **Insert caption**.

### Curated offline aliases

1. In **Import → Name**, type a name and open **▸ Details**. If Details is
   already open, choose **Resolve name**.
2. **CURATED ALIASES · OFFLINE** lists other names for the same structure, each
   with its source and license.
3. Choose **Copy** to copy an alias, or **Use below** to use it as the caption of
   the next insertion. **Use original name below** switches back to the typed
   name.
4. To name a drawn molecule, select it and choose **Generate selected name** in
   the same Details area. **GENERATED SYSTEMATIC NAME · LOCAL RULES** shows the
   result with **Copy name** and **Insert caption**.

Opening Details does not open the preview or insert anything. Changing the
typed name clears the alias choice. An alias caption replaces the molecule's
existing name caption instead of adding a second one.

### Chemical names inspector

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

## Limits

- Ambiguous names, unsupported or ignored stereo, relative or racemic
  qualifiers, radicals, polymers, queries and disconnected salts are rejected
  with a reason. A rejected name leaves the drawing and Undo history unchanged.
- Optical rotation signs are not turned into R/S. The one exception is
  `(+)-lactic acid`, which reads as `(2S)-2-hydroxypropanoic acid`.
- Generated names cover neutral organic molecules with up to 64 heavy atoms.
  Charged or isotope-labeled molecules and fused, spiro or bicyclic parents
  give an error.
- Generated names are systematic. They are not always the preferred IUPAC name.
- The alias catalog matches the exact structure, so stereo, charge and
  isotopes count. **No curated aliases available in the bundled catalog.** only
  means the catalog has no entry, not that the molecule has no other names.
- Other formats keep a caption's text but not its link to the molecule.

## Implementation notes

### Name to structure

The Rust OPSIN port handles supported systematic and retained names, including
substituted functional groups, bicyclic/spiro systems, heterocycles, supported
absolute R/S and E/Z stereo, isotopes and formal charge. The editor accepts a
connected graph with at most 512 atoms. ReShiki verifies the editable graph
before inserting it. The specific retained name `(+)-lactic acid` is normalized
locally to `(2S)-2-hydroxypropanoic acid`, using
[ChEBI:422](https://www.ebi.ac.uk/chebi/CHEBI:422), then parsed and checked
against its explicit S graph. The parser result identifies this retained-name
source. Other optical-rotation names, including derivatives, salts and
conflicting descriptors, remain subject to strict rejection; this entry is not
a general sign-to-configuration rule. The original input can be used for **Add
name below**, while structure-to-name returns the verified
`(2S)-2-hydroxypropanoic acid`. Older screenshots that show this name rejected
predate this change. Changing the input or drawing invalidates pending or
outdated results.

### Structure to name

Abbreviated groups are expanded for naming. Showing an already named molecule
reuses its caption. The **ReShiki organic rules 1** profile covers supported
neutral organic structures with at most 64 heavy atoms. Its declared subset
includes carbon chains with supported branching and unsaturation, simple
cycloalkanes, benzene and selected heteroaromatic parents; acids, simple esters,
primary amides, nitriles, aldehydes, ketones, alcohols, primary amines, ethers
and halogen substituents; and supported parent R/S and E/Z configurations.
Carbon stems are limited to 20 atoms and substituent nesting to three levels.
Unsupported functional groups and stereo contexts return an error. A generated
name appears only after the local Rust parser reconstructs the same graph and
specified stereo. Parent selection and naming rules also have independently
expected tests. Comprehensive IUPAC coverage and preferred IUPAC name (PIN)
selection are not claimed. Unspecified stereo remains unspecified.

### Captions

The caption is ordinary editable text with an optional association to its
complete molecule. It follows layout changes. A chemical identity change
removes an obsolete automatic caption in the same Undo step; Undo restores it.
Editing the caption text or dragging it independently detaches it, preserving
that custom text through later chemical edits. Atom maps and visual atom
numbers do not change name identity.

`.rsk` save, reopen and whole-molecule copy keep the association. Copying a
caption alone keeps plain text. Figure export and CDXML/CDX keep visible
caption text; external formats and older ReShiki readers cannot keep the
association. The optional metadata uses the existing document-version
convention, so an older reader may save the caption as ordinary text.

### Alias catalog

The bundled catalog contains 40 retained or functional-class names for 30
identities, including `ethyl alcohol` for ethanol. Each alias displays its
source, version, record and license. The
[complete catalog and provenance](../src/naming/catalog/README.md) record the
selected OPSIN data (MIT license) and one ChEBI equivalence (CC BY 4.0). The S,
R and unspecified lactic acid graphs have separate entries. Neither charge nor
isotope variants inherit the neutral, unlabeled alias. Reaction map labels do
not change chemical identity. Looking up aliases needs no network connection,
Java, Python or other program. **Resolve name** is disabled while parsing and
for empty input. Changing preview chemistry prevents using aliases from the
original graph. After chemistry changes, generate a new name before using it.

### Records

The [desktop review and fixtures](changes/rust-chemical-naming.md) cover an
earlier build and an older design of the **Chemical names** inspector, with
examples, provenance and reproducible checks. The
[dock and molecule-caption record](changes/molecule-name-label.md) covers the
Import dock and captions, and the [alias review](changes/offline-name-synonyms.md)
covers the catalog.
