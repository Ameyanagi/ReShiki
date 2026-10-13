# Generic structures and finite expansion

Available in Nightly builds; not in ReShiki 0.11.0.

This workflow supports a finite subset of generic structures. The [ring expansion review](changes/generic-ring-expansion.md) records native macOS checks and retained limitations.

ReShiki records choices and repeat definitions in the drawing. **Expand Generic Structures** produces explicit finite combinations in a new tab. An indefinite polymer remains generic until you supply finite counts; it is not treated as an infinite molecule with calculated properties.

## Atom and group alternatives

Select one ordinary atom, press **Enter**, choose **Automatic**, enter a list and choose **Apply · Enter**:

| Input                  | Meaning                                                                   |
| ---------------------- | ------------------------------------------------------------------------- |
| `[C,N]` or `[Cl,Br,I]` | One product per element choice.                                           |
| `[OMe,OPh]`            | Methoxy or phenoxy, both attached through oxygen as real fragment graphs. |
| `[D,T]`                | Deuterium or tritium.                                                     |
| `[NOT Cl,Br,I]`        | Retained exclusion query; cannot be finitely enumerated.                  |

A group alternative needs an isolated atom or one single outside bond. Unknown fragment labels and duplicate choices are refused. `R`, numbered R labels and `X` can be retained as unresolved choices, but must be replaced with concrete choices before expansion. **Text label (dummy atom)** mode creates text rather than these definitions. When reopening an existing generic label, explicitly switch back to **Automatic** to edit its chemical definition.

## Variable substituent positions

Select only the 2–300 candidate atoms. In **Properties → Labels & chemistry → Attachment points**, choose **Variable**, or use **Attachment points → Variable attachment** in the selection context menu. Draw one ordinary single bond from the new `*` handle to the substituent. Expansion removes the handle and attaches that substituent to each candidate position in turn. Independent sites form combinations. **Multi-center** means attachment to all targets and is not enumerated as a position choice.

For a supported detachable substituent, expansion keeps the host ring fixed and
moves the substituent rigidly outward. The new connector uses the drawing
style's normal bond length. An ordinary C/N/O/S root with one single-bond branch
neighbor receives a 120° attachment angle; internal substituent distances stay
unchanged. Existing ordinary peripheral ring substituents are retained.

Placement must clear the preserved host, saved/refreshed atom-label ink and the
new connector. Ambiguous or intersecting placements reject the whole expansion
and retain the source. This is local placement, not a redraw of an internally
crowded fragment. Shared/bridged anchors, unsupported concave hosts and combined
variable/link/repeat placement on the same component remain restricted.

The [follow-up review](changes/generic-ring-expansion.md) records the original
ring-crossing defect, automated checks and native acceptance status.

## Label repetition and polymer units

For one repeated atom, select an atom with exactly two ordinary single connecting bonds, press **Enter**, use **Automatic**, and enter `[CH2]1-3`. Expansion uses one, two or three CH₂ atoms at that position. Ordinary chain and ring link nodes are supported.

For a repeating fragment, draw a bracket enclosing its atoms while leaving terminal groups outside. Select it and use the **REPEATING UNIT** controls:

1. Enter `1-3` in **Repeats** and choose **Head to tail**.
2. Choose **Use enclosed atoms**, or press Enter in the count field. This records atom membership and existing crossing bonds. For a horizontal bracket, the left crossing is the head and the right crossing is the tail.
3. **Clear repeating unit** removes the definition while retaining the graphical bracket.

A positive integer gives one length; `1-3` gives every integer length in the range. Counts must be ordered and between 1 and 1000. `n` retains an indefinite unit. **Head to head** and **Mixed / unknown** are stored but are not finitely expanded. Finite expansion needs one connected ordinary fragment, exactly two single crossing bonds and explicit head/tail ordering. Terminal groups remain outside the replicated fragment. A bracket shape alone has no polymer meaning.

## Expand, undo and save

Clear the selection to expand the entire drawing, or select complete molecular components with their required captions, groups and attachments. Choose **Expand Generic Structures** in the context menu or **Properties → Labels & chemistry → Chemical abbreviations**. Generated link/repeat components receive a new 2D layout.

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

The source tab and its history stay unchanged. All products appear in one new-tab Undo step: Undo empties it and Redo restores it. Rejection installs no partial products. Save source and products separately; reopened files start with fresh history.

Default limits remain 500 raw candidate products, a conservative 100,000-atom raw expansion bound and one aggregate work allowance of 5,000,000 units. Candidate/atom admission happens before duplicate removal: an expansion cannot exceed these caps merely because it might collapse to fewer unique products. Copies, chemistry, layout, display measurement and deduplication share the same work allowance. Large text, paths or pictures can exhaust it even for a small chemical expansion. Invalid valence or a work-limit error in any combination rejects the whole request; no partial output is installed.

Expansion also rejects indefinite counts, unresolved nicknames, exclusion lists, nested/overlapping or directly adjacent units, ambiguous ladder/copolymer connections and specified affected stereochemistry. Mapped repeat atoms and generic structures in explicit reaction membership are restricted. Changed components with generated name captions, attached arrows or rear-opacity scopes require those dependencies to be resolved first; unchanged independent components and captions may be copied. Generic components require expansion before identifiers, properties, derived chemical labels, NMR prediction or joining. Independent ordinary components remain usable.

`.rsk` files keep typed definitions; read the [file compatibility](changes-unreleased.md#file-compatibility) note before you share them with ReShiki 0.11.0 users. Supported CDXML/CDX and V3000 MOL subsets carry represented semantics and refuse unrepresentable data. Fragment alternatives are not ordinary MOL element lists. Full infinite/macromolecular chemistry, ChemDraw generic/polymer parity and a dedicated bulk SDF library workflow remain outside this subset.
