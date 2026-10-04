# Use ReShiki structures and reactions in CAS SciFinder

ReShiki can prepare molecular SMILES and MOL data for a **manual** CAS SciFinder
structure search. Ethanol text entry and a ReShiki-generated MOL import were
verified in an authorized SciFinder session on October 2, 2026; both searches
returned ethanol (CAS RN 64-17-5). An authorized CAS SciFinder account and your
institution's normal access route are required.

The molecule workflow covers **one complete, connected molecule**. The reaction
workflow below uses ChemDoodle JSON with explicit reactant/product roles.
The browser remains responsible for sign-in, reviewing the query and running
the search. ReShiki has no automatic SciFinder search integration.

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
   **OK**, then **Submit Search**. Review CAS's matching and stereochemistry
   settings and confirm the expected substance in the results. The tested
   **As Drawn** search also returned salts and multicomponent substances; it
   was not an exact-only result set. A successful export alone does not
   establish a search match.

For a specified stereoisomer, inspect the **Stereochemistry** filter. In the
tested lactic-acid search, both **Absolute Stereo Match** and **Absolute Stereo
Mirror Image** were selected initially. Keeping only **Absolute Stereo Match**
returned the expected L-lactic-acid entry (CAS RN 79-33-4). Multicomponent
matches still appeared; the filter does not mean the results contain only the
isolated molecule.

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
file. CAS Draw's tested MOL route is a file import; the embedded ChemDoodle
editor also offers MOL text loading under **Open**. Keep the `.rsk` original
for drawing layout and ReShiki editing information.

## Paste a reaction with ChemDoodle JSON

Use this route for a single explicit forward reaction with ordinary supported
reactants and products. Ordinary molecular SMILES/MOL does not carry reaction
roles. In the tested CAS Draw text field, `CCO>>CC=O` was rejected as an
unrecognized format; its upload dialog offered only `.cxf` and `.mol`.

1. In ReShiki, include the whole reaction and its participant molecules in the
   selection, or clear the selection to use the whole drawing. The reaction must
   have explicit reactant/product assignments, not just an arrow graphic.
2. Right-click an object in the selection and choose **Copy as → ChemDoodle
   JSON · reaction**. To copy the whole drawing, right-click blank canvas outside
   the selection bounds. Check that the submenu says **Copy as · whole drawing**.
   Review any warning or error before switching applications.
3. In SciFinder, open **Draw**, choose **ChemDoodle** from the editor selector,
   and select **Reactions**.
4. Choose **Open**. Paste into the field labeled **Or paste MOLFile or
   ChemDoodle JSON text and press Load**, then choose **Load**.
5. Inspect every participant, the forward arrow, and the displayed
   **reactant**/**product** roles. Choose **OK**, then **Submit Search**.
   Confirm the intended transformation in the reaction results.

The initial JSON exporter rejects agents, atom maps, stereochemistry, aromatic
bonds, radicals, special bonds, collapsed abbreviations, disconnected
participants and non-unit coefficients rather than silently dropping their
meaning. Expand defined abbreviations first. It is a bounded search handoff,
not a general reaction interchange format. Keep the native drawing; use
RXN/reaction SMILES for other receivers that support those formats.

The [ChemDoodle JSON specification](https://web.chemdoodle.com/docs/chemdoodle-json-format/)
defines molecular graphs, synthetic arrows and role references. CAS's
[reaction-query guide](https://cas-product-help.zendesk.com/hc/en-us/articles/10275351755533-Reaction-Query-Features)
explains participant roles and optional query features. These references do not
establish that every ChemDoodle feature is accepted by SciFinder.

## Limits to review

| Drawing content                                                                         | Initial handoff scope                                                                                                                           |
| --------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| A complete ordinary molecule                                                            | Start with SMILES or MOL and inspect CAS's imported structure.                                                                                  |
| Tetrahedral or double-bond stereo                                                       | Locally test identity preservation, then verify the CAS depiction and search settings. Unspecified stereo remains unspecified.                  |
| Defined collapsed abbreviations                                                         | The underlying atoms are exported; abbreviated appearance need not survive. Verify the full graph.                                              |
| Plain text labels, R-groups, query atoms, polymers, variable or multicenter attachments | Outside this workflow. A displayed label is not a defined molecule, and CAS query features do not establish ReShiki query-format compatibility. |
| Salts, mixtures or multiple disconnected molecules                                      | Outside the initial single-molecule workflow. Selecting one component changes the substance being described.                                    |
| Reactions and schemes                                                                   | Use the bounded ChemDoodle JSON reaction route above. Ordinary MOL/SMILES does not preserve reaction roles.                                     |
| Coordinate/exotic bonds or MOL V3000                                                    | ReShiki has format-specific handling, but CAS support for every emitted variant has not been verified.                                          |
| Captions, graphics, colors and document layout                                          | These are not an exact molecular identity and are not retained by ordinary molecular identifiers.                                               |

**Copy as** uses selected objects, or the whole drawing when nothing is selected.
The SMILES/MOL keyboard shortcuts require a selection. Neither verifies that the
selected atoms form a complete molecule. Molecular Copy as formats are disabled
for a copied snapshot with explicit reaction metadata; select an individual
participant molecule for this single-molecule workflow.

## Reproducible local preparation

The example below creates four public test drawings and their `.smiles`, `.mol`
and `.inchi` exports: ethanol, a specified lactic-acid stereoisomer,
stereodefined difluoroethene, and a molecule with defined collapsed groups.
It also prepares an ethanol-to-acetaldehyde reaction, with native, RXN and
reaction-SMILES role/connectivity checks, plus the same ChemDoodle JSON text
produced by **Copy as**. It reimports the three molecular formats through
ReShiki and requires matching full InChIKeys,
formulas, heavy-atom counts and heavy-atom bond counts, with no warnings.
InChI import may materialize explicit hydrogens; those counts are recorded
without treating a hydrogen representation change as a chemical identity change.
Reaction RXN/SMILES are checked for roles and connectivity; the JSON is checked
for atoms, bonds, attached hydrogens, arrow and role references. ReShiki does
not import ChemDoodle JSON. Output hashes and
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

On October 2, 2026, the workflows were exercised in Microsoft Edge on macOS
through an authorized institutional SciFinder session. The baseline checks
used public ethanol. CAS Draw accepted `CCO` and the generated `ethanol.mol`,
showed C2H6O (46.07), and returned ethanol (CAS RN 64-17-5) first under
**As Drawn**. The observed result count was 17,602 and included salts and
multicomponent entries; counts can change. No private drawing or credentials
were included in the fixtures or evidence.

The generated lactic-acid SMILES preserved a stereobond; the receiver's
**Absolute Stereo Match** filter returned L-lactic acid (79-33-4). Generated
`F/C=C/F` selected **Double Bond Geometry As Drawn** and returned
trans-1,2-difluoroethylene (1630-78-0) first among 22 results.
The 16-atom fixture with two defined collapsed groups exported its full SMILES
graph. CAS showed C12H17NO3 (223.27) and returned the expected
N-(4-methoxyphenyl)carbamic acid tert-butyl ester (18437-68-8) first among
seven results.

The exact ChemDoodle JSON emitted by the shared **Copy as** preparation path
for `ethanol-oxidation.rsk` was pasted into the receiving editor. Ethanol
appeared as **reactant**, acetaldehyde as **product**, with the forward arrow.
Submitting the search returned 2,070 **As Drawn** reactions. Opening the
1,383-reaction alcohol-oxidation group showed the expected ethanol (64-17-5)
to acetaldehyde (75-07-0) scheme.

The native macOS app was also checked with this fixture. Both **Copy as · whole
drawing** and **Copy as · selected objects** displayed all 11 format choices.
The actual **ChemDoodle JSON · reaction** menu actions copied identical
609-byte payloads, matching the generated fixture. Keyboard paste from the
system clipboard into ChemDoodle loaded the expected roles, and the selected
reaction copy completed the same search through the expected ethanol-to-
acetaldehyde result. Agents, atom maps and reaction stereo were not part of
this fixture.

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
step is this verified manual route plus accessible SMILES/MOL and bounded
ChemDoodle reaction copy controls.
A later direct-search action needs a supported contract, an authorized test
account, institution access requirements, and explicit decisions about chemical
data transmission and complete-molecule selection. It must report successful
preparation separately from successful browser opening or search execution.
