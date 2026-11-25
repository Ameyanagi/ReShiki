# Use ReShiki drawings in an ELN

Electronic lab notebooks (ELNs) and compound registration systems take
structures and reactions as ChemDraw files, and many take only CDX. ReShiki
writes CDX and CDXML files the way ChemDraw does, including the reaction steps
that tell a notebook which molecules are reactants, products and reagents. You
do not need ChemDraw to attach a reaction drawn in ReShiki.

## Choose CDX once

Open **Settings → Copy & export** and set **ChemDraw files → Format** to
**CDX**. From then on ReShiki writes CDX wherever it writes a ChemDraw file
without asking:

- the ChemDraw choice in **Export → Structure** and its **Copy** button;
- the first ChemDraw entry in **Save as…**.

CDXML stays the default. To write the other format for one file, pick it in the
**Structure** menu, or choose the other ChemDraw entry in **Save as…**. A new
drawing still saves as `.rsk` with **Save**; keep that file too, since only
`.rsk` keeps everything.

## Attach a reaction

1. Draw the reaction in one row: reactants, the arrow, then products. Put
   reagents above or below the arrow and type conditions such as `reflux` next
   to it, or link them as [arrow labels](arrows.md).
2. Right-click the arrow and choose **Preview reaction**. The outlines show what
   the file will call reactants, products and reagents. If they are wrong,
   assign the roles yourself in **Reaction roles**; see
   [Reaction roles](reactions.md#define-a-reaction).
3. Open **Export**, choose **CDX drawing** in the **Structure** menu and click
   **Export…**.
4. Attach the `.cdx` file in the notebook's reaction section.

Every arrow with a reaction becomes a ChemDraw reaction step. The step lists the
reactants, products, plus signs, the arrow, reagent structures and condition
text above and below it, and atom maps when the atoms are mapped. A scheme of
several arrows keeps each intermediate as the product of one step and the
reactant of the next.

## If the notebook refuses the file

- **"The file does not contain a reaction".** Check that **Preview reaction**
  finds reactants and products for the arrow. A molecule that crosses the arrow,
  or an arrow or molecule whose purpose is Reference or Illustration, has no
  reaction role. Ungroup the reaction before exporting: RDKit and Indigo, which
  many notebooks build on, read no reaction inside a group.
- **The notebook accepts only CDX.** Set **Format** to **CDX** as above, or pick
  **CDX drawing** for this export.
- **The reaction arrives without some molecules.** Assign the roles by hand. A
  hand-drawn arrow with nothing beside it, or a scheme where one molecule sits
  between two rows, can make detection miss or share a molecule.

ChemDraw 26 opens these files as reactions, and RDKit 2026.09 and Indigo 1.49
read their reactants and products. Individual notebooks were not tested; if
yours still refuses a file, please
[open an issue](https://github.com/Ameyanagi/ReShiki/issues) with the exported
file and the message it shows.

See also [Clipboard and file formats](clipboard.md) and
[Settings](settings.md).
