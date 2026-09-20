# Chemical abbreviations

Properties → Chemical abbreviations provides compact labels while retaining the complete atom/bond graph. Formula, mass, SMILES and molecular identity use the full structure.

- Select one terminal atom and choose a common group such as OMe, Boc, Ph or TBS, then **Replace selected endpoint**. This changes the chemistry and preserves the outside connecting bond.
- **Contract common groups** finds supported groups in the selected atoms, or the whole drawing when nothing is selected.
- Select a connected fragment with at most one outside bond, enter a name and optional reversed label, and choose **Contract selection**. This changes presentation only.
- **Expand selected** or **Expand all** restores the internal atoms for editing. A collapsed group selects, moves, copies and deletes as a unit. Individual atom-property changes require expansion.

Each action is one Undo step. Native version-10 documents preserve custom names and reversed labels. SVG, PDF and PNG display the abbreviated drawing. Editable binary/XML exchange carries explicit nested chemical fragments; imported labels can expand back to real atoms. Common reversed labels are recognized on import. Custom reversed spellings are native-only.

The current subset supports 29 common presets and one attachment per abbreviation. Nested or multiple-attachment nicknames are unsupported. Editable export rejects abbreviated groups carrying internal atom indicators rather than losing those indicators; expand them first. Labels do not introduce invented elements or replace the underlying structure with text.

Verification covers native save/reopen, reversed labels, selection/copy/delete/Undo, full molecular identity through cleanup and editable exchange, all 29 preset definitions, and an independently saved native binary fixture. Desktop exchange displayed MeO–phenyl–NH–Boc and expanded its chemical labels to the full 16-atom structure.
