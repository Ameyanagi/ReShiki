# Molecular identity

Available in Nightly builds; not in ReShiki 0.16.0.

**Properties → Identity** puts the name, formula, molecular weight and chemical
identifiers together. Click a value to copy it. **Descriptors** keeps the other
calculated properties in a collapsed card.

## Use it

1. Select the atoms or bonds to analyze. With nothing selected, Identity uses
   the whole drawing. Double-click an atom to select its complete molecule.
2. Open **Properties → Identity**. Click **Formula**, **Mol. Wt.**, **SMILES**,
   **InChI**, **InChIKey**, or the elemental-analysis value to copy it. Hover to
   see a long value in full. Copying keeps the drawing and selection unchanged.
3. Click **Name it** to generate a name for one complete supported molecule or
   charge-balanced salt. When a curated common name is available, it appears
   above the systematic name. Click either name to copy it.
4. Choose **Add name below** to place the displayed name with the existing
   molecule link. Undo removes that placement.
5. Open **Name options** to type a custom name or return to the automatic name.
   A custom name stays visible when the chemistry changes, with a warning.
6. Open **Descriptors** for exact mass, cLogP, TPSA, hydrogen-bond counts, rings,
   unpaired electrons, molecular classes, **Predict NMR…**, and **Recalculate**.
   The existing property pins and caption decimal-place choices stay available.

## Limits

- A partial selection keeps its fragment formula and identifiers, including
  recalculated implicit hydrogens at cut bonds. Generate a name or place a name
  only after selecting a complete supported molecule.
- Attachment points and crystals retain their composition notices. Identity
  does not present ordinary molecular identifiers for those scopes.
- Unavailable identifiers cannot be copied. Clipboard access can be refused;
  the status reports success only after the clipboard confirms the write.
- Names are generated on request. A changed drawing or selection cannot reuse
  a generated name for another molecule. Existing custom names and linked
  property captions keep their current file representation.

## Implementation notes

The card reads the revision-, file-epoch-, and atom-scoped Properties analysis.
Local naming uses the existing bounded name generator and identity validation.
The original caption, alias-caption and linked-property actions handle
placement. Clipboard text is frozen when requested and published through the
existing native transport or the browser clipboard promise; completion belongs
to its originating tab. This change adds no document fields or file version.
