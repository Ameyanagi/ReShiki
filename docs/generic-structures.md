# Generic structures and finite expansion

This workflow is an unreleased supported subset under review. Current native desktop acceptance is pending; this guide is not a release or CI certification.

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

## Label repetition and polymer units

For one repeated atom, select an atom with exactly two ordinary single connecting bonds, press **Enter**, use **Automatic**, and enter `[CH2]1-3`. Expansion uses one, two or three CH₂ atoms at that position. Ordinary chain and ring link nodes are supported.

For a repeating fragment, draw a bracket enclosing its atoms while leaving terminal groups outside. Select it and use the **REPEATING UNIT** controls:

1. Enter `1-3` in **Repeats** and choose **Head to tail**.
2. Choose **Use enclosed atoms**, or press Enter in the count field. This records atom membership and existing crossing bonds. For a horizontal bracket, the left crossing is the head and the right crossing is the tail.
3. **Clear repeating unit** removes the definition while retaining the graphical bracket.

A positive integer gives one length; `1-3` gives every integer length in the range. Counts must be ordered and between 1 and 1000. `n` retains an indefinite unit. **Head to head** and **Mixed / unknown** are stored but are not finitely expanded. Finite expansion needs one connected ordinary fragment, exactly two single crossing bonds and explicit head/tail ordering. Terminal groups remain outside the replicated fragment. A bracket shape alone has no polymer meaning.

## Expand, undo and save

Clear the selection to expand the entire drawing, or select complete molecular components with their required captions, groups and attachments. Choose **Expand Generic Structures** in the context menu or **Properties → Labels & chemistry → Chemical abbreviations**. Products retain each explicit combination; symmetry-equivalent choices are not deduplicated. Generated link/repeat components receive a new 2D layout.

The source tab and its history stay unchanged. All products appear in one new-tab Undo step: Undo empties it and Redo restores it. Rejection installs no partial products. Save source and products separately; reopened files start with fresh history.

Default limits are 500 products, 100,000 total output atoms and one aggregate work allowance of 5,000,000 units covering copies, chemistry, layout and display measurement. Large text, paths or pictures can exhaust it even for a small chemical expansion. Invalid valence in any combination rejects the whole request.

Expansion also rejects indefinite counts, unresolved nicknames, exclusion lists, nested/overlapping or directly adjacent units, ambiguous ladder/copolymer connections and specified affected stereochemistry. Mapped repeat atoms and generic structures in explicit reaction membership are restricted. Changed components with generated name captions, attached arrows or rear-opacity scopes require those dependencies to be resolved first; unchanged independent components and captions may be copied. Generic components require expansion before identifiers, properties, derived chemical labels, NMR prediction or joining. Independent ordinary components remain usable.

Native `.rsk` saves retain typed definitions and use document version 26; older readers must be upgraded. Supported CDXML/CDX and V3000 MOL subsets carry represented semantics and refuse unrepresentable data. Fragment alternatives are not ordinary MOL element lists. Full infinite/macromolecular chemistry, ChemDraw generic/polymer parity and a dedicated bulk SDF library workflow remain outside this subset.
