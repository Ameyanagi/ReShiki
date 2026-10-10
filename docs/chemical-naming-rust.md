# Local chemical naming

This feature is under review and is not included in ReShiki 0.11.0. Its Rust
backend is introduced separately in [PR #291](https://github.com/Ameyanagi/ReShiki/pull/291).

Open **Import → Name** to turn a supported chemical name into an editable
structure. Right-click a molecule and choose **Show chemical name** to place a
verified systematic name below it. Both directions use local deterministic Rust rules. Naming requires
no Java, Python, naming service or network connection.

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

The [earlier native review](changes/rust-chemical-naming.md) retains screenshots
of the previous separate naming panel. Those images do not represent the
compact Import dock.

The Rust OPSIN port handles supported systematic and retained names, including
substituted functional groups, bicyclic/spiro systems, heterocycles, supported
absolute R/S and E/Z stereo, isotopes and formal charge. The editor accepts a
connected graph with at most 512 atoms. Ambiguous names, ignored or unsupported
stereo, relative/racemic qualifiers, radicals, polymers, queries and
disconnected salts reject with an explanation. Optical rotation alone, such as
`(+)-lactic acid`, does not establish an R/S configuration.

## Generate a name from a structure

1. Right-click a molecule and choose **Show chemical name**. The clicked
   connected molecule is the target even when other molecules are selected or
   grouped with it. Abbreviated groups are expanded for naming.
2. ReShiki generates and verifies its systematic name locally. For ethanol the
   result is `ethan-1-ol`, centered below the molecule's visible drawing.
3. Choose **Hide chemical name** from the same menu to remove it. Show and Hide
   each form one Undo step. Showing an already named molecule reuses its caption.

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
