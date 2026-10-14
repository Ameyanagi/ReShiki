# Generic structures and finite expansion

Available in Nightly builds; not in ReShiki 0.11.0.

Generic structures keep element and group lists, variable attachment points
and repeat counts in the drawing. **Expand generic structures** turns them into
every concrete structure and opens all of them together in one new tab.

## Use it

### List elements or groups at one atom

1. Select one ordinary atom and press **Enter**.
2. Choose **Automatic** and type a list:

   | Input                  | Meaning                                        |
   | ---------------------- | ---------------------------------------------- |
   | `[C,N]` or `[Cl,Br,I]` | One product per element.                       |
   | `[OMe,OPh]`            | Methoxy or phenoxy, both attached through O.   |
   | `[D,T]`                | Deuterium or tritium.                          |
   | `[NOT Cl,Br,I]`        | Any atom except these. Kept, but not expanded. |

3. Choose **Apply · Enter**.

To edit an existing list later, open the atom again and switch back to
**Automatic**.

### Let a substituent take several positions

1. Select only the candidate atoms (2–300).
2. In **Properties → Labels & chemistry → Attachment points**, choose
   **Variable**. You can also right-click and choose **Attachment points →
   Variable attachment**.
3. Draw one single bond from the new `*` handle to the substituent.

Expansion attaches the substituent to each candidate in turn. **Multi-center**
attaches to all selected atoms at once and is not expanded into positions.

### Repeat one atom

1. Select an atom with exactly two single bonds and press **Enter**.
2. Choose **Automatic** and type, for example, `[CH2]1-3`.

Expansion makes one product each with one, two and three CH₂ groups.

### Define a repeating unit

1. Draw a bracket around the repeating atoms. Leave the end groups outside.
2. Select the bracket. In the **Repeating unit** section, type `1-3` in
   **Repeats** and choose **Head to tail** under **Pattern**.
3. Choose **Define repeating unit**, or press Enter in **Repeats**.

A number gives one length; `1-3` gives every length from 1 to 3. Use `n` for an
indefinite polymer. **Clear repeating unit** removes the definition; the
bracket stays.

### Expand

1. Clear the selection to expand the whole drawing. To expand only some
   molecules, select them completely, with their captions, groups and
   attachments.
2. Choose **Expand generic structures** in the **Generic structure** section of
   **Properties**. When the selection is part of a generic structure, the
   right-click menu offers the same command.
3. All products open together in one new tab, and duplicates are removed.
   Undo in the new tab removes all products; Redo restores them. The source tab
   and its Undo history do not change.

Save the source and the products as separate `.rsk` files. Read the
[file compatibility](changes-unreleased.md#file-compatibility) note before you
share them with ReShiki 0.11.0 users.

## Limits

- Expansion stops if it would create more than 500 structures or 100,000 atoms
  (counted before duplicates are removed), or if it needs too much work. Large
  text, paths and pictures count toward that work. If any product fails,
  nothing is added.
- `R`, numbered R labels and `X` can stay as placeholders, but expansion needs
  concrete choices. `[NOT …]` lists, `n` counts and the **Head to head** and
  **Mixed / unknown** patterns are kept but not expanded.
- Identifiers, properties, chemical labels, NMR prediction and joining wait
  until you expand. Ordinary molecules in the same drawing still work.
- Expansion keeps the ring in place and moves the substituent outward. A
  crowded or ambiguous placement rejects the whole expansion and keeps the
  source.
- A bracket alone has no polymer meaning. An indefinite polymer stays generic;
  it is not treated as an infinite molecule.
- ChemDraw generic and polymer parity and a bulk SDF library workflow are not
  part of this feature.

## Implementation notes

### Choices and positions

A group alternative needs an isolated atom or one single outside bond.
`[OMe,OPh]` choices are real fragment graphs. Unknown fragment labels and
duplicate choices are refused. **Text label (dummy atom)** mode creates text
rather than these definitions.

For a supported detachable substituent, expansion keeps the host ring fixed and
moves the substituent rigidly outward. The new connector uses the drawing
style's normal bond length. An ordinary C/N/O/S root with one single-bond branch
neighbor receives a 120° attachment angle; internal substituent distances stay
unchanged. Existing ordinary peripheral ring substituents are retained.
Independent sites form combinations.

Placement must clear the preserved host, saved/refreshed atom-label ink and the
new connector. Ambiguous or intersecting placements reject the whole expansion
and retain the source. This is local placement, not a redraw of an internally
crowded fragment. Shared/bridged anchors, unsupported concave hosts and combined
variable/link/repeat placement on the same component remain restricted. The
[ring expansion review](changes/generic-ring-expansion.md) records the original
ring-crossing defect and its checks.

### Repeats

Ordinary chain and ring link nodes are supported for repeated atoms. **Define
repeating unit** records atom membership and the existing crossing bonds. For a
horizontal bracket, the left crossing is the head and the right crossing is the
tail. Counts must be ordered and between 1 and 1000. Finite expansion needs one
connected ordinary fragment, exactly two single crossing bonds and explicit
head/tail ordering. Terminal groups remain outside the replicated fragment.
Generated link/repeat components receive a new 2D layout.

When the bracket's two sides cross exactly two bonds, the unit also includes
the side atoms attached to the enclosed atoms, such as C=O oxygens or ring atoms
above and below the bracket. Units with heteroatoms, such as polyesters and
polyethers, expand like carbon chains. A condensed label such as `OCH2CH2` on
one atom is text, not atoms, and cannot be expanded; draw each atom of the unit.

### Duplicates

Expansion retains one product per unique normalized chemical graph, keeping the
first representative in stable enumeration order. Coordinates, drawing
orientation and generated object IDs do not create a different chemical product.
Comparison preserves elements, isotopes, charge, atom maps, stereochemistry and
directed dative bonds. Existing restrictions on affected stereochemistry apply.

| Finite variable-position example                                           | Unique products                                 |
| -------------------------------------------------------------------------- | ----------------------------------------------- |
| Six equivalent benzene positions for one substituent                       | One monosubstituted product.                    |
| Five available positions on benzene with one different fixed substituent   | Three products: ortho, meta and para.           |
| Positions not related by a label-preserving symmetry on an asymmetric ring | The distinct chemical products remain separate. |

Explicit hydrogen vertices may remain distinct from an attached-hydrogen
representation; universal equivalence across those representations is not
promised.

### Limits and rejection

Default limits are 500 raw candidate products, a conservative 100,000-atom raw
expansion bound and one aggregate work allowance of 5,000,000 units. Candidate
and atom limits apply before duplicate removal: an expansion cannot exceed these
caps merely because it might collapse to fewer unique products. Copies,
chemistry, layout, display measurement and deduplication share the same work
allowance. Invalid valence or a work-limit error in any combination rejects the
whole request; no partial output is installed. Reopened files start with fresh
history. Drawing layout is the largest cost: a PET unit (14 heavy atoms with
its C=O groups and ring) expands as a drawing up to n = 5, and a short
polyester unit up to n = 10, before the work allowance is spent.

Expansion also rejects unresolved nicknames, nested/overlapping or directly
adjacent units, ambiguous ladder/copolymer connections and specified affected
stereochemistry. Mapped repeat atoms and generic structures in explicit reaction
membership are restricted. Changed components with generated name captions,
attached arrows or rear-opacity scopes require those dependencies to be resolved
first; unchanged independent components and captions may be copied.

### Files

`.rsk` files keep typed definitions. Supported CDXML/CDX and V3000 MOL subsets
carry represented semantics and refuse unrepresentable data. Fragment
alternatives are not ordinary MOL element lists.
