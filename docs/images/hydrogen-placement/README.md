# OH/HO documentation captures

These are unmodified window screenshots of the existing hydrogen-placement controls, captured on 2026-09-27. They illustrate a documentation addition, not an application behavior change.

- Application: published ReShiki v0.8.0, tag commit `9229ce3`, macOS Apple Silicon, isolated settings profile.
- Documentation base: `d4312f4a04a8e803551f5f6c12d0f6c9491b1269`.
- Drawing style: JACS / ACS, Arial 10 pt; canvas zoom 216% in both captures, with matching window size and framing.
- Input: [editable sugar drawing](../../../website/public/examples/hydrogen-placement.rsk), imported from `OC[C@H]1O[C@@H](O)[C@H](O)[C@@H](O)[C@@H]1O` through the app's Import command.
- Target: the topmost hydroxyl oxygen (atom ID 6); all other atoms are outside the selection.

To reproduce, open the drawing, select the topmost oxygen with Select / move, and open **Properties → Labels & chemistry → Atom labels & numbering…**. Keep **Selected atoms** as the scope and **Show implied hydrogens** enabled.

- `oh-right.png`: Hydrogen position is **Right**, displaying OH. This is the saved example's starting state.
- `ho-left.png`: Hydrogen position is **Left**, displaying HO. The unsaved-change marker records this display edit.

The controls were operated in the desktop app. Left, Right, restoring Automatic, and Undo were checked. The native document comparison confirms that only atom 6's hydrogen-position override changes; atom coordinates, bonds, and the other hydroxyl labels are retained.

Release-note caption: “The User Guide now shows how to switch OH/HO placement for an individual hydroxyl.”
