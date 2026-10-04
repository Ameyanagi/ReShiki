# 3D optimization and keyboard drawing

**3D optimize…** generates a molecular conformer and lets you adjust its
projection before applying it. **Keyboard drawing (F8)** supplies an explicit
atom or bond hotspot so contextual drawing shortcuts work without pointer
hover. Both commands are available with Select or Lasso; commands that do not
fit in the context row are in its **⋯** menu.

## Preview an optimized molecule

1. Select an atom, a bond, or the whole molecule, then choose **3D optimize…**.
   A partial selection still includes its entire connected molecule. With no
   selection, the drawing must contain exactly one molecular component.
2. Wait for the initial conformer. Three mutually exclusive buttons choose
   **MMFF94**, **MMFF94s**, or **UFF**; the selected button is checked and the
   default is **MMFF94s**. If generation fails,
   choose another field to regenerate, or use **Generate** to retry the current
   field. Missing parameters
   produce an error; ReShiki does not silently change the chosen field.
3. Use **Start** to enable live local relaxation. Drag an unpinned atom to move
   it while the rest of the molecule relaxes around the target. Dragging retains
   that atom's current displayed depth. **Stop** stops calculation and keeps the
   preview. You can move targets while stopped, then press **Start** to relax
   from the latest target. A converged live session shows **Live relaxation
   ready** and waits for another edit.
4. **Shift-click** atoms to add or remove them from the selection, then choose
   **Pin selected**. Pins hold exact physical XYZ positions during relaxation.
   Use **Unpin selected** before dragging a pinned atom, or **Clear pins** to
   release them all. A partial selection made before opening the preview is
   pinned at its newly generated positions; selecting the whole molecule starts
   with no pins.
5. Rotate with the **3D tilt** tool by dragging an atom or inside a ring, or use
   the preview bar's **↶**, **↷**, **Tilt up**, **Tilt down**, and **Roll**.
   Switch back to Select for atom dragging and Shift-click selection. Rotation
   changes the viewing projection while retaining physical pins and energy.
   **Show original** lets you compare the untouched drawing.
6. Choose **Apply** to validate and commit the displayed projection as one Undo
   step. **Cmd/Ctrl+Z** restores the original drawing; Redo restores the applied
   result. **Cancel** discards the entire preview. Escape cancels an active atom
   drag first; with no active canvas gesture, it also cancels the preview.

The committed document stays unchanged until Apply completes. Ordinary graph
edits are paused during a preview. Pan and zoom remain available; switching
tabs pauses that tab's calculation. Apply rejects invalid coordinates, extreme
bond stretching, atom overlaps, and inversion or degeneration of specified
tetrahedral or double-bond stereochemistry.

## Force fields and calculation scope

The backend reuses RDKit **2026.03.6**, pinned to revision
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. ReShiki calls its ETKDGv3 conformer
generator and MMFF94, MMFF94s, and UFF implementations; these force fields are
not replacement spring models implemented in the editor. The source and
distribution details are in [Native 3D geometry distribution](geometry-backend.md).

Generation uses a bounded, seeded conformer ensemble, minimizes the valid
candidates with the chosen field, and selects their lowest energy result.
Subsequent dragging performs bounded local minimization. This searches nearby
conformations rather than guaranteeing a global minimum. The displayed energy
is in kcal/mol; compare it within the same molecule and force field.
After switching fields in a stopped preview, press **Start** to recalculate.

The supported calculation domain is one connected covalent component with at
most **512 original atoms** and **2048 bonds**. Supported elements are
**H, B, C, N, O, F, Si, P, S, Cl, As, Se, Br, and I**, with valid valence and
available parameters for the selected field. Formal charges are limited to
−8…+8. Single, double, triple, and aromatic bonds, ordinary tetrahedral stereo,
and alkene stereo are supported. Query or variable atoms, radicals,
coordination and multi-centre attachments, hydrogen-bond contacts, partial and
quadruple bonds, and enhanced or non-tetrahedral stereo are rejected.

Calculation-only hydrogens are added on a detached graph and retained through
relaxation. Original atoms keep their IDs and order. These temporary hydrogens
are not inserted into the drawing by Apply. The resulting projection keeps the
editable document objects and specified chemical stereo; unspecified centres
do not acquire new document stereo assignments.

## Automatic depth, frozen gray, and original ink

**Automatic depth** is initially enabled in the preview. Rear atom labels,
bonds, and ring fills fade toward the canvas background as the projection
rotates; black ink appears gray on a light canvas. Base colors remain editable.
Depth appearance changes paint, independently of force-field calculation and
chemical bond styles.

Turning **Automatic depth** off freezes the current fade. It keeps the XYZ
projection and visible colors; further rotation does not recalculate the fade.
**Clear depth paint** removes the fade and restores the base colors while
keeping the current projection. Re-enable Automatic depth to make the fade
follow depth again. These preview changes are committed only by Apply.

After Apply, Select/Lasso offers **Enhance depth**, **Freeze depth**, and
**Clear depth** when applicable. Their **⋯** menu labels are **Enhance depth
appearance**, **Freeze depth appearance**, and **Clear depth appearance**.
They operate on complete selected molecular components, or all components
when nothing is selected. **Original ink** / **Use original ink for selection**
lets selected atoms and their connecting bonds use their base ink within an
otherwise faded molecule. Native documents retain the editable depth
appearance; figure and clipboard output use the visible paint.

## Draw with an explicit keyboard hotspot

Press unmodified **F8**, or choose **Keyboard drawing**, to enter. A visible
marker and the context row identify the active atom or bond. One selected atom
becomes the hotspot; two selected bonded atoms make that bond active. Otherwise
the nearest visible atom or bond to the view centre is used. An empty drawing
starts at the view centre.

Letters and digits reuse the existing [atom shortcuts](contextual-shortcuts.md#at-an-atom)
and [bond shortcuts](contextual-shortcuts.md#at-a-bond). Their chemical mappings
are not new shortcuts; Keyboard drawing adds an explicit target, empty-drawing
seeding, and predictable focus after each edit.

On an empty drawing, **1** creates the first C–C bond; **3**, **6**, or **7**
starts the corresponding ring.
Ring attachment keeps the attachment hotspot; arrows let you choose an atom or
bond for the next edit. Navigation does not add bonds or move the drawing.

For example, on a new blank drawing, **F8 → n → 1 → 1** creates an N–C–C chain
with the final carbon active. Use arrows back along the chain, then **0** to
branch from the chosen atom. To close an open chain, navigate to one endpoint,
press **[**, navigate to the other endpoint, and press **]**. Self-connections,
duplicate bonds, and rejected valence changes leave the drawing unchanged.
Committed edits are Undo steps; Undo/Redo also restore the hotspot and ring
closure mark associated with the edit.

Pointer hover cannot steal the active hotspot. Text fields, atom-label drafts,
and dialogs retain their typing and caret keys; close or finish them before
continuing canvas commands. Entering Keyboard drawing releases focus from its
opener. Cmd/Ctrl and Alt shortcuts retain their existing meanings, including
Undo, crossing order, and selection transforms. Lowercase and uppercase remain
distinct. **Shift+3–8** still selects ring tools. Outside Keyboard drawing,
arrows retain the normal selection nudge behavior and contextual keys continue
to use pointer/selection targets.

## Reviewer reproduction: keyboard targets and history

Use a new drawing for each empty-seeding case. Leave the pointer over empty
space or an unrelated atom to verify that the marker owns the keys. Except for the draft
case, start with text fields and dialogs closed. In navigation cases, choose
the arrow pointing toward the next visible target; Left, Right, Up, and Down
use the same rule and prefer connected targets.

| Keys                         | Target/setup and short reproduction                                                                  | Expected result                                                                                                                                              |
| ---------------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| F8                           | Start with one atom selected; repeat with a selected bond's two endpoints, then with a blank drawing | Enter at that atom, that bond, or the view centre respectively; the marker and Keyboard drawing row appear                                                   |
| F8 / Escape                  | With Keyboard drawing active and no draft open, press F8; re-enter and press Escape                  | Each leaves the mode without changing the graph; Done (F8) is the matching button                                                                            |
| n; 1; 3 / 6 / 7              | In separate blank drawings, enter with F8 and press the shown existing shortcut                      | Create N; a C–C bond with its new endpoint active; or the corresponding ring, with no mouse placement                                                        |
| Arrow / [ / ] / Enter        | Enter with F8 on a blank drawing before seeding an atom                                              | No atom, bond, label draft, or Undo entry is created; the status explains the missing target                                                                 |
| Arrow                        | Seed **n → 1 → 1**, then press an arrow toward the previous atom twice                               | Atom → intervening bond → atom; positions and connectivity do not change                                                                                     |
| Shift+arrow                  | From the same chain, navigate first from an atom, then from a bond                                   | Atom → atom or bond → bond; the other target kind is skipped, without a drawing edit                                                                         |
| 1                            | At an active atom with available valence, press 1                                                    | Add the next chain atom; its endpoint becomes the hotspot                                                                                                    |
| 0                            | Navigate back to an atom with available valence and press 0                                          | Add a branch; the original atom remains the hotspot for another branch                                                                                       |
| Enter                        | Seed a ring with 3, leaving its ring selection intact, then press Enter                              | Open only the active atom's label draft; Escape closes the draft and keeps Keyboard drawing active                                                           |
| [                            | At an open-chain endpoint, press [; repeat at a bond target                                          | Mark the atom without editing the graph; a bond target only shows the atom-target hint                                                                       |
| ]                            | After [, navigate to the other nonbonded endpoint with available valence and press ]                 | Add one single bond, keep the current atom active, and clear the mark; missing marks, self/duplicate connections, or invalid valence do not edit the drawing |
| Cmd/Ctrl+Z; Cmd/Ctrl+Shift+Z | Undo the ] connection, then Redo; Windows also accepts Ctrl+Y                                        | Undo restores the open graph and prior mark/hotspot; Redo restores the closed graph and cleared mark                                                         |
| Cmd/Ctrl+Z; Cmd/Ctrl+Shift+Z | Undo/Redo a 1 growth or 0 branch                                                                     | Restore the graph and hotspot belonging to each committed edit; navigation and marking alone add no Undo entries                                             |

For the optimizer review, select a whole molecule and open **3D optimize…**;
check each of the three force-field buttons, **Start/Stop**, Shift-selection
and **Pin selected**, Select ↔ 3D tilt, and **Automatic depth** off versus
**Clear depth paint**. Finish once with **Cancel** and once with **Apply →
Undo → Redo**. Expected behavior is described above; these are reproduction
steps, not additional platform-validation claims.

ChemDraw's contextual hotkeys informed this workflow; see Revvity's
[ChemDraw tips and tricks guide](https://revvitysignals.com/sites/default/files/2024-05/rs-e-book-chemdraw-signals-chemdraw-tips-and-tricks-eBook.pdf).
ReShiki's F8 entry, empty-drawing seed, branch focus, and **[ / ]** connection
contract are defined here. They are not a claim of exact ChemDraw emulation.
