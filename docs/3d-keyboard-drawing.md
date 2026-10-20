# 3D optimization and keyboard drawing

**3D optimize… (Cmd/Ctrl+Shift+D)** generates a molecular conformer and lets you adjust its
projection before applying it. Select and Lasso combine mouse and keyboard
drawing by default: a visible hotspot is the target for contextual letters,
digits, and arrow navigation. **v** draws a three-member ring, and **l** enters
Cl at an atom or empty hotspot. Choose Select/Lasso with the tool buttons;
**Escape** cancels the current operation or returns to Select. **F8** switches
the current tab between hotspot drawing and classic contextual controls.
Commands that do not fit in the context row are in its **⋯** menu.

## Show a molecule as a 3D model

Select one molecule and choose **2D**, **3D** or **Both** on the small
**Show as** bar above the selection, or at the top of **Properties → Structure**
(also **Show as** in the right-click menu and **Show as 3D model** in the
command palette). The bar shows while a whole molecule is selected with Select
or Lasso and hides during any drag; it sits above the selection box and its
rotation handle, or below them at the top of the window, and when neither fits
it stays inside the canvas over the selection. **3D** draws the molecule as a
ball & stick model in place of its drawing, **Both** puts the model one bond length to its right, and
**2D** shows the drawing again. The first switch generates the conformer in the
background with the same worker as 3D optimize (fresh ETKDG conformers, then
MMFF94s); a result for a drawing that changed meanwhile is discarded. Later
switches reuse the stored model, and each switch is one Undo step. When a model
first appears (2D → 3D or 2D → Both), a 0.75 s morph moves the atoms from the
drawing into 3D while the hydrogens grow out of their atoms. It only redraws
the canvas and never delays input; ReShiki has no reduced-motion setting yet.

The model links to the molecule by atom ID. It never changes chemistry, atom IDs
or the 2D layout, which is kept for switching back. Clicking or lassoing the
model selects the molecule, so moving, aligning, copying and Duplicate carry it
along; while the molecule is shown in 3D, its hidden 2D atoms cannot be picked.
A chemical edit to the molecule (element, charge, bonds or stereo) removes the
model.

The model sits in a square frame that holds it in any rotation, so the frame,
its handles and the bar stay put while the model turns. On the canvas, a model
that is not selected moves like any object. Once its molecule is selected:

- **drag inside the frame** to rotate it like a trackball (0.01 radian per
  pixel about the screen axes);
- **Shift-drag** to roll it about the view axis;
- **double-click** to reset the view to the opening principal-axes view;
- drag **within 8 px of the frame edge**, or with Alt or Cmd/Ctrl, to move or
  copy it as usual;
- drag a **corner handle** to resize the frame, which scales the model about
  the opposite corner and leaves the 2D drawing as it is. Edge handles are
  hidden for a selection of shown models, and a selection mixing a shown model
  with other objects has no resize handles.

Each rotation, reset and resize is one Undo step, and the canvas preview is the
committed result.

The **3D tilt** tool, its X/Y step commands (right-click → **3D tilt**, the
Alt+Shift+arrow keys and the command palette) and the **Tilt X / Tilt Y**
transform fields turn a molecule shown only in 3D by turning its model, with
the same rotation as dragging inside the frame: vertical movement turns it
about X and horizontal movement about Y, at the tilt's 0.5° per pixel, and
Shift snaps to 15°. Its hidden 2D drawing stays as it is. In a selection
that also holds drawn atoms or shapes, the model turns and the rest tilts,
together as one Undo step, and the preview is the committed result. A
molecule shown as **Both** tilts its visible 2D drawing and leaves the model
alone.

The selection box's rotation handle, **Rotate 180°**, the Alt/Option+arrow
rotate steps and the **Rotate** transform field roll a model shown only in 3D
about the view axis, clockwise as they turn a drawing, and its frame stays in
place; the other selected objects turn as before, in the same Undo step, and
the preview is the committed result. Editing never changes the hidden 2D
drawing of such a molecule: Flip, which would make the model the other
enantiomer, Scale/W/H, Clean up, 3D optimize, reference align/stretch and Join
say **Switch to 2D** instead, a mixed selection flips just its other objects,
and rings, joins and fused templates never attach to the hidden atoms.

### 2D layout

While a model is shown, **Properties → Structure → 2D layout** chooses what the
**2D** buttons return to. **Original drawing** (the default) restores the
layout you drew. **From the 3D view** flattens the model as it is turned now
into the 2D drawing, like 3D optimize's projection: atoms keep their IDs and
drawn stereo, the drawing keeps its bond-length scale and centre, and far bonds
are lighter through **Automatic depth**. The right-click **Show as** menu and
the command palette also offer **2D from the 3D view** directly. Each flatten is
one Undo step that also switches the molecule to 2D; the model is kept, and
shown in 3D again it is back in the same frame, at the same place and size.

The choice belongs to each model and is saved with it. A flattened model also
keeps the layout you drew (positions, depth, stored stereo and depth
appearance), so **Original drawing**, or **Show as → 2D drawing**, brings it
back, also after saving and reopening. Choosing **Original drawing** brings it
back at once, in the same Undo step, so while that choice shows, the 2D atoms
(and the 2D half of **Both**) are always the layout you drew. Choosing the display that is already
shown changes nothing.

## Preview an optimized molecule

The preview opens the right **Properties** panel. Force-field, pin, view, and
depth controls scroll there; **Apply** and **Cancel** remain at its bottom.
The canvas toolbar stays one row, with a **3D preview →** link that reopens the
controls if the panel is hidden or showing another tab.

1. Select an atom, a bond, or the whole molecule, then press **Cmd+Shift+D** on
   macOS or **Ctrl+Shift+D** on Windows/Linux, or choose **3D optimize…**.
   A partial selection still includes its entire connected molecule. With no
   selection, the drawing must contain exactly one molecular component.
2. Wait for the initial conformer. Three mutually exclusive buttons choose
   **MMFF94**, **MMFF94s**, or **UFF**; the selected button is checked and the
   default is **MMFF94s**. If generation fails,
   choose another field to regenerate, or use **Generate 3D** to retry the current
   field. Missing parameters
   produce an error; ReShiki does not silently change the chosen field.
3. Use **Start relaxation** to enable live local relaxation. Drag an unpinned atom to move
   it while the rest of the molecule relaxes around the target. Dragging retains
   that atom's current displayed depth. **Stop relaxation** stops calculation and keeps the
   preview. You can move targets while stopped, then press **Start relaxation** to relax
   from the latest target. A converged live session shows **Live relaxation
   ready** and waits for another edit.
4. **Shift-click** atoms to add or remove them from the selection, then choose
   **Pin selected**. Pins hold exact physical XYZ positions during relaxation.
   Use **Unpin** before dragging a pinned atom, or **Clear pins** to
   release them all. A partial selection made before opening the preview is
   pinned at its newly generated positions; selecting the whole molecule starts
   with no pins.
5. Rotate with the **3D tilt** tool by dragging an atom or inside a ring, or use
   the preview controls **↶**, **↷**, **Tilt up**, **Tilt down**, and **Roll**.
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

The backend uses the **Rust COSMolKit 0.5.0** implementations of ETKDGv3,
MMFF94, MMFF94s, and UFF, with its exact source revision pinned by Cargo.
The installed application is one executable and requires no Python or RDKit
runtime. The independent development comparison uses RDKit **2026.03.6**;
the embedded parameter sources originate from RDKit **2026.03.1**. The source
and distribution details are in [Rust 3D geometry distribution](geometry-backend.md).

Generation uses a bounded, seeded conformer ensemble, minimizes the valid
candidates with the chosen field, and selects their lowest energy result.
When a drawing retains valid, genuinely three-dimensional coordinates, those
coordinates are optimized first. Flat drawings, including tilted planes, and
coordinates that conflict with the specified chemistry use conformer generation.
Fullerene cages such as C₆₀ use a short initial trial and a distance-geometry
fallback without flat-geometry assumptions when that trial fails. This changes
the starting geometry; the chosen MMFF/UFF force field and stereo validation
still apply to the resulting molecule. Closed carbon cages must also stay
inside the supported near-convex geometry envelope; a folded or strongly
nonconvex cage produces a diagnostic instead of being applied. C₆₀ has a full
regression fixture; this does not establish support for every larger fullerene.
An unsuccessful ensemble dominated by planarity rejections may use that
alternate initialization for other strained structures too. If embedding times
out, ReShiki may retry with a single conformer. This reduces sampling rather
than changing the chosen force field. Retries remain bounded. The preview
identifies existing 3D geometry, cage or alternate starting geometry, and
single-conformer recovery.
Subsequent dragging performs bounded local minimization. An unchanged set of
targets and pins pauses after 50 batches or eight batches without meaningful
energy improvement. The preview stays editable; moving a target or changing
pins resumes live calculation, and **Start** deliberately retries. **Stop**
keeps calculation stopped until Start. This searches nearby
conformations rather than guaranteeing a global minimum. The displayed energy
is in kcal/mol; compare it within the same molecule and force field.
After switching fields in a stopped preview, press **Start** to recalculate.

The supported calculation domain is one connected covalent component. Available
memory determines admission of **128–640 original atoms**, with at most four
times as many original bonds and **4,096 all-atom coordinates**, including
temporary hydrogens. Unknown memory observations retain the 512-atom / 2,048-bond
envelope. Larger structures remain editable. See the
[calculation resource policy](chemistry-resource-policy.md) for independent
safety ceilings and time limits. Supported elements are
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
**Clear depth** removes the fade and restores the base colors while
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

The [Rear opacity control](rear-opacity.md) (ReShiki 0.12.0 and later)
separately makes rear ink lighter or transparent from
**Properties → 3D appearance**. Its default 100% preserves opaque paint.
**Clear depth** also restores rear opacity to 100% while retaining XYZ.

## Draw with mouse and keyboard

Keyboard drawing is **on by default** in Select and Lasso, including new blank
drawings. The marker and context row show the active atom, bond, or empty
position. No F8 activation is needed. An initial single selected atom or bond's
two endpoints provide the target; otherwise the nearest visible atom or bond
to the view centre is used. A blank drawing initially targets the view centre.

Actual mouse motion or a canvas click transfers the hotspot to the pointed
atom, bond, or empty position. Arrow navigation and keyboard edits then retain
that hotspot until the pointer moves or is clicked again. A stationary pointer
does not pull the hotspot back to an older hover target. Moving to empty canvas
also provides a position where a shortcut can start another structure.

Letters and digits reuse the existing [atom shortcuts](contextual-shortcuts.md#at-an-atom)
and [bond shortcuts](contextual-shortcuts.md#at-a-bond). Their chemical mappings
are not new shortcuts. On empty canvas, an atom letter creates the first atom,
**1** creates a C–C bond, and **3 / 6 / 7** starts phenyl / a six-member / a
five-member ring. **v** attaches or fuses a three-member ring, or seeds one on
empty canvas. **l** enters Cl at an atom or empty hotspot; at a bond it places
the double line on the left. These contextual commands take priority over
the classic **v / l** Select/Lasso aliases. Use the tool buttons to choose
Select or Lasso, or Escape to cancel/return to Select.

For example, on a new blank drawing, **n → 1 → 1** creates an N–C–C chain with
the final carbon active. Arrows navigate atom → bond → atom; **Shift+arrows**
skip the other target kind, moving atom → atom or bond → bond. Navigation does
not edit the drawing. **1** at an atom grows the chain and advances to its new
endpoint; **0** adds a branch while keeping its origin active. Ring attachment
keeps the attachment hotspot. **Enter** opens the active atom's label draft,
even after a ring operation selected several atoms.
At a bond or empty hotspot, Enter opens Properties. To name a selected fragment
with **Contract selection**, turn keyboard drawing off with F8 first; classic
Enter edits an existing selected abbreviation or contracts a multi-atom
selection. With hotspot drawing on, the active atom takes priority over the
selection.

For a coordination contact, mark a donor N/O/S/P with **[**, navigate to a transition metal and use **Coordinate }** or **}**. This adds a directed dative contact without moving atoms; **]** keeps its ordinary covalent checks. See [coordination contacts](fragment-joining.md#coordination-contacts-in-place) for scope and projection styling.

To close an open chain, navigate to one endpoint and press **[**, then navigate
to the other endpoint and press **]**. This adds a single bond and clears the
mark. Self-connections, duplicate bonds, and rejected valence changes leave the
drawing unchanged. Committed edits are Undo steps; Undo/Redo also restore the
hotspot and connection mark associated with the edit.

Press unmodified **F8** or turn off **Keyboard drawing** in the controls for
this tab. This restores classic pointer/selection contextual shortcuts and arrow
nudging: arrows move a selection by 1 drawing unit, or 10 with Shift. The opt-out
survives tool and tab changes; new tabs start with keyboard drawing on. F8 or
the **Keyboard drawing** command turns it back on and returns to Select.
With keyboard drawing off, clear the selection and point to empty canvas to
use **v / l** for Select/Lasso. At a classic atom or bond target, their existing
chemical actions still apply.
**Escape** retains ordinary cancel/return-to-Select behavior and preserves the
tab's on/off choice; it is not a persistent off command.

Other drawing tools suspend the hotspot and clear a pending connection mark.
Return to Select or Lasso to resume automatically, unless this tab was opted
out. Text fields, atom-label drafts, and dialogs retain their typing and caret
keys; finish or cancel them before canvas shortcuts. Cmd/Ctrl and Alt retain
their usual modified commands. Lowercase and uppercase remain distinct, and
**Shift+3–8** still selects ring tools.

## Reviewer reproduction: keyboard targets and history

Use a new drawing for each empty-seeding case, with Select or Lasso active and
no draft or field focused. Do not press F8 to start. Once a keyboard step has
chosen the hotspot, leave the pointer still until testing a deliberate mouse
transfer. In navigation cases, choose the arrow pointing toward the next
visible target; Left, Right, Up, and Down prefer connected targets.

| Keys / gesture                  | Target/setup and short reproduction                                                                     | Expected result                                                                                                                                   |
| ------------------------------- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| n; 1; 3 / 6 / 7                 | In separate new blank drawings, press the shown existing shortcut directly                              | Create N; a C–C bond with its new endpoint active; or the corresponding ring, without F8 or mouse placement                                       |
| Arrow / [ / ] / Enter           | On a blank drawing, try these before seeding                                                            | No graph or Undo entry is created; Enter opens Properties, while navigation and connection keys need a target                                     |
| Arrow                           | Seed **n → 1 → 1**, then press an arrow toward the previous atom twice                                  | Atom → intervening bond → atom; positions and connectivity do not change                                                                          |
| Shift+arrow                     | From the same chain, navigate first from an atom, then from a bond                                      | Atom → atom or bond → bond; the other kind is skipped without editing                                                                             |
| Mouse motion / click            | Move to or click an atom, a bond, then empty canvas                                                     | The hotspot transfers to that atom, bond, or drawing position                                                                                     |
| Stationary pointer              | Move onto one atom, use Shift+arrow to another, then type n without moving the pointer                  | The keyboard-chosen atom changes to N; the previous hover does not steal the target                                                               |
| 1 / 0                           | At an atom with available valence, press 1; navigate back and press 0                                   | 1 advances to the new endpoint; 0 leaves the branch origin active                                                                                 |
| v / l                           | Try v at an atom, a bond, or an empty hotspot; try l at an atom or empty hotspot, then at a double bond | v attaches/fuses/seeds a three-member ring; l enters Cl or places the double line left; neither selects a tool while its chemistry action applies |
| Enter / Escape                  | Seed a ring with 3 and press Enter with its ring selection intact; press Escape                         | Only the active atom's label draft opens; Escape closes the draft and keyboard drawing remains on                                                 |
| F8; Enter                       | Select several uncontracted atoms, turn keyboard drawing off, then press Enter                          | Classic Contract selection opens; a fully selected existing abbreviation instead opens its label draft                                            |
| [                               | At an open-chain endpoint, press [; repeat at a bond target                                             | Mark the atom without an edit; a bond target only shows the atom-target hint                                                                      |
| ]                               | After [, navigate to the other nonbonded endpoint with available valence and press ]                    | Add one single bond, keep the current atom active, and clear the mark; rejected connections do not edit the graph                                 |
| Cmd/Ctrl+Z; Cmd/Ctrl+Shift+Z    | Undo/Redo a connection, growth, or branch; Windows also accepts Ctrl+Y                                  | Restore the graph, hotspot, and mark for each committed edit; navigation and marking alone add no Undo entries                                    |
| F8 / Keyboard drawing control   | Turn keyboard drawing off, select an atom, and use Arrow / Shift+arrow                                  | Classic 1 / 10 unit nudging returns; tool and tab changes retain this tab's opt-out                                                               |
| v / l with keyboard drawing off | Clear the selection and point to empty canvas, then press v or l                                        | Classic Select/Lasso tool aliases work; classic atom/bond contextual chemistry remains available at its targets                                   |
| F8                              | From an opted-out tab, press F8 again                                                                   | Return to Select and re-enable hotspot drawing for that tab                                                                                       |
| Another tool / Escape           | With keyboard drawing on, choose a bond or text tool, then press Escape                                 | The other tool suspends the hotspot; Escape returns to Select and resumes it without changing the on/off preference                               |
| Escape                          | Press Escape in Select once with keyboard drawing on, and once after F8 turned it off                   | Ordinary cancellation preserves each state; Escape does not toggle keyboard drawing                                                               |

For the optimizer review, select a whole molecule and open **3D optimize…**;
check each of the three force-field buttons, **Start/Stop**, Shift-selection
and **Pin selected**, Select ↔ 3D tilt, and **Automatic depth** off versus
**Clear depth paint**. Finish once with **Cancel** and once with **Apply →
Undo → Redo**. Expected behavior is described above; these are reproduction
steps, not additional platform-validation claims.

ChemDraw's contextual hotkeys informed this workflow. Its
[ChemDraw 21 manual](https://chem.beloit.edu/classes/programs/ChemDraw_21_manual.pdf)
documents arrow hotspot navigation with a selection tool and assigns **F8** to
**View → Reduce** (zoom out). ReShiki assigns F8 to the per-tab keyboard drawing
toggle. ChemDraw remains the reference for contextual chemistry; ReShiki's
default hotspot lifecycle, empty-canvas seeding, branch focus, and **[ / ]**
connection commands follow the contract above. This is not an exact ChemDraw
clone. Revvity's [ChemDraw tips and tricks guide](https://revvitysignals.com/sites/default/files/2024-05/rs-e-book-chemdraw-signals-chemdraw-tips-and-tricks-eBook.pdf)
provides further contextual shortcut examples.
