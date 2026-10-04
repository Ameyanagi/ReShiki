# Use a ReShiki molecule in CAS SciFinder

ReShiki can prepare SMILES, InChI and MOL data for a **manual** CAS SciFinder
structure search. CAS documents these input formats. Review the imported structure and search
settings before submitting a query. An authorized CAS
SciFinder account and your institution's normal access route are required.

The initial workflow covers **one complete, connected molecule** and a substance
search. The browser remains responsible for sign-in, reviewing the query and
running the search. ReShiki has no direct SciFinder search integration.

## Copy SMILES

1. In ReShiki, select the whole molecule, including any defined abbreviations.
   Double-click a molecule with the selection tool to select its connected
   atoms. Check that other molecules are not selected. A partial selection can
   export a different chemical fragment.
2. Right-click the selection and choose **Copy as → SMILES**, or press
   **Cmd/Ctrl+Alt+C**. Review any reported export warning or error;
   an unsuccessful copy can leave the previous clipboard content in place.
3. Open [CAS SciFinder](https://scifinder-n.cas.org/) through your usual
   institution access route. Open **Draw** and choose **Substances** in its
   structure editor.
4. Paste the SMILES into CAS Draw's text-to-structure field, then use
   **Add to Editor** or Enter. CAS's guide uses keyboard paste for this field.
5. Inspect the resulting atoms, charges, isotopes and stereochemistry. Choose
   **OK**, then **Submit Search**. Review CAS’s matching and stereochemistry
   settings and confirm the intended substance in the results. A successful export alone does not establish a search match.

CAS's [structure-search guide](https://cas-product-help.zendesk.com/hc/en-us/articles/14956691547149-Using-a-Structure-in-Your-Search)
documents SMILES and InChI text entry. Its
[ChemDraw import guide](https://cas-product-help.zendesk.com/hc/en-us/articles/9894081037965-Import-a-Structure-File-from-ChemDraw)
shows the receiving field and keyboard-paste procedure. ReShiki supplies ordinary
structure text; it does not use the ChemDraw add-in. CAS also describes exact and
similarity search in its
[advanced structure-search guide](https://cas-product-help.zendesk.com/hc/en-us/articles/48611610535949-Advanced-Structure-Search-Techniques).

## Import MOL instead

In ReShiki, choose **Export → Structure & exchange → MOL structure → Export
MOL…** for a drawing containing just the intended molecule. File export uses
the whole drawing. In CAS Draw, use its import control,
**Choose File**, select the `.mol` file and confirm it. Review the structure
before searching. CAS documents the file-picker route in
[Import a Structure File in CAS Draw](https://cas-product-help.zendesk.com/hc/en-us/articles/9893983568525-Import-a-Structure-File-in-CAS-Draw).

**Cmd/Ctrl+Alt+O** copies MOL **text** from the selection. It does not create a
file, and CAS's documented MOL route is a file import. Keep the `.rsk` original
for drawing layout and ReShiki editing information.

## Limits to review

| Drawing content                                                                         | Initial handoff scope                                                                                                                                        |
| --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| A complete ordinary molecule                                                            | Start with SMILES or MOL and inspect CAS's imported structure.                                                                                               |
| Tetrahedral or double-bond stereo                                                       | Locally test identity preservation, then verify the CAS depiction and search settings. Unspecified stereo remains unspecified.                               |
| Defined collapsed abbreviations                                                         | The underlying atoms are exported; abbreviated appearance need not survive. Verify the full graph.                                                           |
| Plain text labels, R-groups, query atoms, polymers, variable or multicenter attachments | Outside this workflow. A displayed label is not a defined molecule, and CAS query features do not establish ReShiki query-format compatibility.              |
| Salts, mixtures or multiple disconnected molecules                                      | Outside the initial single-molecule workflow. Selecting one component changes the substance being described.                                                 |
| Reactions and schemes                                                                   | Outside scope. Ordinary MOL/SMILES does not preserve reaction roles. Do not interpret reaction export availability as a verified SciFinder reaction handoff. |
| Coordinate/exotic bonds or MOL V3000                                                    | ReShiki has format-specific handling, but CAS support for every emitted variant has not been verified.                                                       |
| Captions, graphics, colors and document layout                                          | These are not an exact molecular identity and are not retained by ordinary molecular identifiers.                                                            |

**Copy as** uses selected objects, or the whole drawing when nothing is selected.
The SMILES/MOL keyboard shortcuts require a selection. Neither verifies that the
selected atoms form a complete molecule. Molecular Copy as formats are disabled
for a copied snapshot with explicit reaction metadata; select an individual
participant molecule for this single-molecule workflow.
