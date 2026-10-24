# Chemical abbreviations

The bundled groups and templates are the current implementation. Their coverage, labels and layouts may be extended or revised in future releases. Existing examples can be saved as editable native drawings.

Select a collapsed group or an atom label and open the Style bar's alignment
menu: **Automatic** (the default, checked) follows the bond direction, as
ChemDraw's Text menu does, and **Left**, **Center**, **Right** and **Stacked
above** force a placement; choosing Automatic again restores it. On an element
label, Left reads OH and Right HO. Multiple labels can change in one Undo step;
with captions in the selection the menu shows Left / Center / Right / Justify.
Press **Enter** to edit the label or its chemical/text meaning; the dialog's
**Alignment** list sets the same choice while you type.
Above places a single nickname above the attachment. Automatic internal labels
such as CCl₂ or NMe keep the attachment element at the bond junction and place
the remaining part according to the bond angles. General multiline formula-token
stacking is not implemented. Native/CDXML/CDX preserve the
override, and Undo/Redo restores it. See the [illustrated PR review](changes-pr17.md).

Properties → Chemical abbreviations provides compact labels while retaining the complete atom/bond graph. Formula, mass, SMILES and molecular identity use the full structure.

- Select one terminal atom and choose a common group such as OMe, Boc, Ph, TBS, TBDPS or OTBDPS, then **Replace selected endpoint**. This changes the chemistry and preserves the outside connecting bond.
- **Contract common groups** finds supported groups in the selected atoms, or the whole drawing when nothing is selected. A group only claims atoms whose charge, unpaired electrons, isotope and hydrogen count match its definition, so a charged or labelled atom stays visible. When a larger group cannot apply because a smaller group takes its outside atom, the smaller candidates are used instead (MeNHBoc contracts to Boc and Me).
- Select a connected fragment whose outside bonds all meet the same selected atom, enter a name and optional reversed label, and choose **Contract selection**. This changes presentation only.
- **Expand selected** or **Expand all** restores the internal atoms for editing. A collapsed group selects, moves, copies and deletes as a unit. Individual atom-property changes require expansion.

Each action is one Undo step. Native version-10 documents preserve custom names and reversed labels. SVG, PDF and PNG display the abbreviated drawing. Editable binary/XML exchange carries explicit nested chemical fragments; imported labels can expand back to real atoms. Common reversed labels are recognized on import. Custom reversed spellings are native-only.

The catalog holds 359 groups with 52 alternative spellings. Typing one of these
labels on an endpoint creates the real group, so formula and mass match the
expanded structure. A reverse spelling, such as MeO, AcO, BnO, TBSO or
(HO)2B, also makes its group (OMe, OAc, OBn, OTBS, B(OH)2), which then
shows whichever spelling suits the side its bond leaves from. NC and OCN are
read from the bond: on a left-hand end (NC–R) they make CN and NCO, but on a
right-hand end or a vertical bond they would mean an isocyanide or a cyanate,
which have no catalog group, so they stay text. A group typed
over a label that reads to the left keeps that side, so OBn typed on the HO
of a sugar template reads BnO, and a group retyped over another group keeps
its alignment. A user group
whose label is spelled like a bundled reverse spelling takes precedence.
Labels are case-sensitive, so MOrph is text and not Morph. Full names, such
as morpholino, 2-thienyl or acetyl, stay text; the label editor suggests
the label to type instead ("Did you mean Morph?").

| Category         | Groups | Examples                                                                    |
| ---------------- | -----: | --------------------------------------------------------------------------- |
| Protecting group |     79 | Boc, Cbz, Fmoc, Alloc, Teoc, PMB, DMB, DMTr, Mtt, SEM, THP, Dde, ivDde, Acm |
| Carbonyl         |     42 | CHO, COMe, CO2tBu, CO2Bn, CONH2, CONMe2, CON(OMe)Me, OSu                    |
| Sulfonyl         |     34 | Ts, Ns, pNs, Pbf, Mtr, SO2Ph, SO3H, Tf, OTf, NTf2                           |
| Nitrogen         |     31 | NMe2, NEt2, NHMe, NHPh, NCO, NCS, Morph, Pip, Pyrr                          |
| Alkyl            |     27 | sBu, iBu, cPr, cHex (Cy), neoPent, Vinyl, Prenyl, Propargyl                 |
| Heteroaryl       |     25 | 2-Py, 3-Py, 4-Py, 2-Thienyl, 2-Furyl, Im, Bt, PT, BT                        |
| Aryl             |     24 | Mes, Dipp, Tripp, PMP, oTol, C6F5, 4-CF3C6H4, 1-Naph                        |
| Halogen          |     20 | CF3, CHF2, OCF3, SCF3, SF5, C2F5, CCl3                                      |
| Silyl            |     19 | TMS, TES, TBS (TBDMS), TIPS, TBDPS, OTMS, CCTMS                             |
| Phosphorus       |     15 | PPh2, PCy2, PtBu2, P(O)Ph2, P(O)(OEt)2, PPh3+                               |
| Sulfur           |     14 | SMe, SPh, SAc, SBn, SOMe, SePh                                              |
| Metal            |     11 | SnBu3, SnMe3, ZnCl, MgCl, GeMe3                                             |
| Alkoxy           |     10 | OMe, OEt, OPh, OtBu, OiPr                                                   |
| Boron            |      8 | Bpin, B(OH)2, Bnep, Bcat, BMIDA, Bdan, BF3-                                 |

The bundled catalog file lists every label with its full names. Phthaloyl
(Phth) bonds twice to its nitrogen; use NPhth for the phthalimido group. TBDPS
(tert-butyldiphenylsilyl) attaches through silicon; OTBDPS attaches through
oxygen and displays as TBDPSO when the parent bond is on its right. Groups can
also be entered with **Enter** in Automatic or Chemical abbreviation mode. Ac
and Ts are also element symbols, so Automatic mode types actinium and
tennessine; choose Chemical abbreviation for acetyl and tosyl. Expanding TBDPS or OTBDPS
reveals the real Si(phenyl)₂–tert-butyl or O–Si(phenyl)₂–tert-butyl structure.

Internal groups can have several
external bonds when they all connect to the same attachment atom. Editable
import requires an explicit definition and connection ordering. CCl₂, CF₂ and
NMe were checked through editable CDX and CDXML round trips, including the real
atoms behind each label. Connections to different atoms within one collapsed
group remain unsupported.

Cp and Cp* additionally retain full cyclopentadienyl/pentamethylcyclopentadienyl ligand graphs and typed multi-center attachment points, contributing C5H5− and C10H15−. Formula counts exclude the handles; coordination valence and complex identifiers remain unsupported. Editable CDXML/CDX expands these haptic groups when nested attachment definitions cannot survive resaving. Nested nicknames remain unsupported. Editable export rejects abbreviated groups carrying internal atom indicators rather than losing those indicators; expand them first. Labels do not introduce invented elements or replace the underlying structure with text.

Verification covers native save/reopen, reversed labels, selection/copy/delete/Undo, full molecular identity through cleanup and editable exchange, every catalog definition, and an independently saved native binary fixture. Desktop exchange displayed MeO–phenyl–NH–Boc and expanded its chemical labels to the full 16-atom structure.

## The group catalog

All typed and detected groups come from one file,
[`catalog.json`](https://github.com/Ameyanagi/ReShiki/blob/main/crates/model/src/chemistry/abbreviations/catalog.json).
Each entry gives the label, an optional reversed label, a SMILES string whose
first atom is the `*` attachment point, the substituent formula, a category and
full names:

```json
{
  "label": "OTBS",
  "reverse": "TBSO",
  "smiles": "*O[Si](C)(C)C(C)(C)C",
  "formula": "C6H15OSi",
  "category": "silyl",
  "names": ["tert-butyldimethylsilyl ether"]
}
```

ReShiki reads each SMILES when it loads the catalog. It derives the atoms and bonds, the
matching rules for **Contract common groups** and the formula from it. The
layout comes from the same 2D depiction that SMILES import uses. Adding a
group is one new entry; no separate geometry file is needed. The optional
`aliases` list adds other typed spellings, such as TBDMS for TBS. Set
`"contract": false` for a group that should only be created by typing, never
found by **Contract common groups**. Groups cannot carry stereochemistry,
isotopes, radicals or `[H]` atoms, and attach through one single bond. Every
label, alias and reverse label must be unique.

When groups overlap, **Contract common groups** prefers the larger group, for
example NHBoc over Boc. Groups of the same size are tried in catalog order.

Of the bundled groups, the protecting, silyl, sulfonyl, boron, tin and
phosphorus groups and the original 62 are found by **Contract common groups**.
Skeleton groups added later, such as rings, heteroaryls, amines, amides and
alkyl chains, are typed only, so contracting a drawing does not hide its
skeleton. A group that duplicates another's structure under a different label,
such as COMe for Ac or CO2tBu for Boc, is also typed only.

## Your own groups

You can add groups of your own. They work like the bundled ones: typing the
label on an endpoint creates the real atoms, and **Contract common groups**
finds them.

### Define a group from the drawing

1. Draw the group attached to any atom, for example the group on a methyl carbon.
2. Select the atoms of the group only. The selection must have exactly one
   single bond to the rest of the drawing; that bond becomes the attachment.
   You can instead draw a `*` atom on a single bond and include it in the
   selection as the attachment point.
3. Open **Properties → Labels & chemistry → Chemical abbreviations**. Under
   **Name a selected fragment**, enter the label and, optionally, the label
   to show when the bond leaves on the right.
4. Choose **Define abbreviation from selection**.

ReShiki saves the group to your abbreviations file and shows the selection
collapsed under the new label, in one Undo step. Undo restores the drawing; the
saved group stays in the file. Saving a label again replaces the earlier group
with that label. Groups with stereochemistry, more than 31 atoms or more than
one attachment cannot be saved.

### The abbreviations file

Your groups live in `abbreviations.json` in the ReShiki data folder:

| System  | Location                                                                             |
| ------- | ------------------------------------------------------------------------------------ |
| macOS   | `~/Library/Application Support/dev.reshiki.ReShiki/abbreviations.json`               |
| Windows | `%LOCALAPPDATA%\reshiki\ReShiki\data\abbreviations.json`                             |
| Linux   | `$XDG_DATA_HOME/reshiki/abbreviations.json`, by default in `~/.local/share/reshiki/` |
| Any     | `$RESHIKI_DATA_DIR/abbreviations.json` when `RESHIKI_DATA_DIR` is set                |

It uses the same format as the bundled catalog, so you can also edit it by
hand or share it:

```json
{
  "format": 1,
  "groups": [
    {
      "label": "OTBDPS2",
      "reverse": "TBDPS2O",
      "smiles": "*O[Si](c1ccccc1)(c1ccccc1)C(C)(C)C",
      "formula": "C16H19OSi",
      "category": "user",
      "names": ["my silyl ether"]
    }
  ]
}
```

Only `label` and `smiles` are required. `smiles` must start with the single
`*` attachment point, joined by a single bond. Labels are 1–32 characters without spaces and cannot be an element symbol or a spelling ReShiki builds itself (N3, MgBr, Cp, OCH3). If you give
a `formula`, ReShiki checks it against the SMILES. `aliases`, `names`,
`category` and `"contract": false` work as in the bundled catalog.

ReShiki reads the file when it starts. A group with the same label as a bundled
group replaces it. An entry that cannot be used, such as an invalid SMILES or a
formula that does not match, is skipped. The status bar then names the first
problem and the file; the other groups still load. Drawings never depend on the
file: a saved drawing keeps every atom of a group, so it opens the same way on a
computer without your groups. For the same reason, generic alternative lists such as `[OMe,OBn]` accept only bundled groups and always use their bundled definitions.
