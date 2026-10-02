# Use a ReShiki molecule in CAS SciFinder

ReShiki can prepare SMILES, InChI and MOL data for a **manual** CAS SciFinder
structure search. CAS documents these input formats, but an authenticated
ReShiki-to-CAS import and search has not yet been verified. An authorized CAS
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
   the exact-structure option offered by the current CAS interface and complete
   the search there. A successful export alone does not establish a search match.

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
the whole drawing. Alternatively, prepare the local fixtures below. In CAS Draw,
use its import control,
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

## Reproducible local preparation

The example below creates four public test drawings and their `.smiles`, `.mol`
and `.inchi` exports: ethanol, a specified lactic-acid stereoisomer,
stereodefined difluoroethene, and a molecule with defined collapsed groups.
It reimports every export through ReShiki and requires matching InChIKeys,
formulas, atom counts and bond counts, with no warnings. Output hashes and
identities are written to `manifest.json`. These are local interchange checks,
not independent chemical-reference or CAS acceptance tests.

Run from the repository root, with a new output directory whose parent exists:

```sh
cargo run --example scifinder_handoff_qa -- /tmp/reshiki-scifinder-handoff
```

For the receiving-application check, start with `ethanol.rsk` in ReShiki and
compare its copied SMILES with `ethanol.smiles`. In an authorized CAS session,
exercise both keyboard entry and `ethanol.mol` import. Record the editor,
browser/platform, date, imported structure and substance-search outcome.
Repeat the stereo and abbreviation cases only after ethanol works. Record
errors and differences rather than broadening the compatibility claim.

On October 2, 2026, the public CAS help page and its text-entry screenshot were
inspected in Microsoft Edge on macOS. Opening the canonical SciFinder address
reached the CAS SSO page with an empty username field; verification stopped
there. No structure or credentials were submitted, and the licensed editor and
search results were not exercised.

## Direct integration decision

CAS documents a
[ChemDraw-specific search integration](https://cas-product-help.zendesk.com/hc/en-us/articles/9894121439245-How-do-I-perform-a-SciFinder-structure-search-within-ChemDraw),
which requires an authorized SciFinder user. This does not define a public
structure-query URL or API for arbitrary desktop applications.
[CAS Connections](https://www.cas.org/solutions/cas-connections) directs customers
building custom API or MCP integrations to CAS Custom Services; access depends
on subscriptions and integration arrangements. No applicable ReShiki API
contract or entitlement has been established.

For [issue #95](https://github.com/Ameyanagi/ReShiki/issues/95), the useful first
step is this documented manual route plus accessible SMILES/MOL copy controls.
A later direct-search action needs a supported contract, an authorized test
account, institution access requirements, and explicit decisions about chemical
data transmission and complete-molecule selection. It must report successful
preparation separately from successful browser opening or search execution.
