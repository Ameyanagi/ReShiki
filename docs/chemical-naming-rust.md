# Local chemical naming

This feature is under review and is not included in ReShiki 0.11.0. Its Rust
backend is introduced separately in [PR #291](https://github.com/Ameyanagi/ReShiki/pull/291).

Open **Import → Chemical names…** to turn a supported chemical name into an
editable structure, or generate a systematic name from a complete selected
molecule. Both directions use local deterministic Rust rules. Naming requires
no Java, Python, naming service or network connection.

## Parse a chemical name

1. Enter a name, such as `ethanol`, and choose **Parse name locally**.
2. Review the structure and its atom/bond count. You can drag preview atoms to
   adjust the layout, or edit its SMILES and choose **Update preview** to change
   the chemistry. **Restore parsed structure** returns to the original graph.
3. Choose **Insert editable structure**. The whole molecule is inserted as one
   Undo step. Undo removes it; Redo restores it.

Changing the preview's chemistry can make the original input name inaccurate.
The panel identifies this change. Scroll the sidebar beside the editable
preview to reach its lower controls.

![Local Rust parsing produces an editable ethanol preview](images/chemical-naming-rust/ethanol-local-parse-preview.jpg)

The Rust OPSIN port handles supported systematic and retained names, including
substituted functional groups, bicyclic/spiro systems, heterocycles, supported
absolute R/S and E/Z stereo, isotopes and formal charge. The editor accepts a
connected graph with at most 512 atoms. Ambiguous names, ignored or unsupported
stereo, relative/racemic qualifiers, radicals, polymers, queries and
disconnected salts reject with an explanation. Optical rotation alone, such as
`(+)-lactic acid`, does not establish an R/S configuration.

## Generate a name from a structure

1. Select a complete connected molecule, including all its bonds. Abbreviated
   groups are expanded to their full chemical graph for naming.
2. Choose **Generate name locally**. For ethanol the result is `ethan-1-ol`.
3. Use **Copy name**, or **Insert caption** to add the name as editable text.
   Inserting a caption is one Undo step and preserves the molecule.

![A complete selected ethanol graph produces ethan-1-ol locally](images/chemical-naming-rust/ethanol-native-generated-name.jpg)

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

Changing the input or drawing invalidates pending or outdated results. Generate
the name again before using a caption after a chemical edit.

See the [native review and fixtures](changes/rust-chemical-naming.md) for the
tested build, examples, provenance and reproducible checks.
