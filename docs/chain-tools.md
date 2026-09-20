# Chain drawing and bond constraints

Added 2026-09-20. **JACS / ACS remains the default**: Arial 10 pt, black drawing color, 14.4 pt bonds and 0.6 pt lines. Straight and snaking chain tools share the same document, chemistry and export pipeline as individually drawn bonds.

## Drawing workflow

- Choose **Straight chain** (`X`) and drag to grow a zigzag. The preview counts new carbon atoms and bonds. Set **Atoms** for an exact size, or clear it to **Auto** so drag distance controls size. Counts include existing attachment endpoints. Clicking places the chosen size; Auto click places six atoms.
- Choose **Snaking chain** (`Shift+X`) to steer while dragging, or hold Ctrl during a straight-chain drag to begin bending. Retrace an earlier vertex to remove the tail. The **Max atoms** field caps a snaking gesture; it does not force a short drag to create the full maximum.
- Start on an atom to extend its molecule. Ending near another atom reuses that atom when it is close to the final generated point. Starting and ending attachment atoms retain their IDs and elements. A click at an existing chain end continues the zigzag along its existing direction.
- Shift changes the starting zigzag side. For a straight chain it changes the whole preview; for a snaking chain it sets the first turn. Overlapping an intermediate atom shows a red preview and leaves the document unchanged on release.
- Escape, losing window focus, or releasing outside the canvas cancels. Retracing a drag all the way to its start also cancels. The entire chain is one history edit; Undo/Redo removes/restores all its new atoms and bonds together.

## Constraints and defaults

The contextual controls independently enable **Length** and **Angles**. Bond direction snapping uses 15° increments; chain interiors default to 120°. Hold Alt to temporarily release both constraints. Existing-atom attachment takes precedence over nominal bond length at the attachment itself.

The length field accepts publication points, from 1 to 300 pt. The chain angle accepts 1–179°. Straight chains can change length continuously when Length is off; with both constraints off their final endpoint follows the pointer. Snaking chains grow toward pointer events at the preferred step size and can use a shorter final step with Length off. Very acute angles or crowded geometry can produce overlaps that need another direction or a larger bond length.

The **JACS / ACS** control restores 14.4 pt length, 120° chain angle and both constraints. It changes tool settings, not the existing drawing. Startup and **every new document** restore JACS/ACS drawing and typography defaults, including after another document used custom settings. Tool preferences are session state, not document-wide style definitions; saved object geometry and formatting persist. Cleanup still uses the shared JACS/ACS preset.

## Grouped molecules

New atoms remain in the groups of a molecule they extend. Joining separately grouped molecules unites overlapping groups while retaining their captions and integral-group status. Properly nested groups remain nested where possible. This also applies to ordinary bond edits and template growth through the common editing path. Native and CDXML tests cover joining two grouped, captioned fragments without splitting the resulting molecule across group boundaries.

## Reference and verification

Desktop checks exercised fixed constraints, free drawing, exact-count insertion and multi-segment snaking. An interchange fixture contains 16-, 6- and 20-carbon chains with a 14.40 pt bond length and coordinate rounding below 0.01 pt.

Moruno desktop verification:

1. Drew a chain in one drag; Check returned `C6H14`, six atoms and five bonds. Reading the saved native file confirmed each bond is 42 world units, or 14.4 pt.
2. Reopened it, set Atoms to four and clicked an endpoint. The original endpoint was reused and the result was a continuous nine-carbon chain, `C9H20`.
3. Changed the preferred length to 20 pt, then used the JACS/ACS reset. The field returned to 14.4 without modifying existing objects.
4. Drew a snaking path with multiple pointer segments and captured the live **10 new C · 9 bonds** preview. Release produced those exact counts. One Undo removed the whole chain and Redo restored it.
5. Checked, fitted and saved the combined two-component drawing: `C19H42`, 19 atoms and 17 bonds, all with 14.4 pt lengths.
6. In the packaged app, selected Times New Roman, 18 pt, bold, red text and 20 pt bonds, then created a new document from the focused length field. The controls returned to Arial, 10 pt, normal black text and 14.4 pt bonds; the `X` drawing shortcut worked immediately afterward. Open, Save and Save As were also exercised from that field without inserting their command letters into its value. The file shortcuts are handled before widget text input.

Current validation: **83 Rust tests and 23 Python tests**, plus formatting and Clippy. New tests cover independent constraints, exact sizes up to 512 atoms, variable length, snaking and retracing, fast pointer events, preview/commit identity, native/MOL/CDXML persistence, group growth, Undo/Redo and JACS defaults after New. Local desktop evidence is under ignored `artifacts/chain-qa-20260920/`.

## Remaining limits

- Chains create carbon atoms connected by ordinary single bonds. Element replacement, bond-order editing and Check remain separate operations. Geometry checks do not replace full valence validation.
- A gesture is limited to 512 chain atoms. There is no viewport auto-pan while drawing, global routing around obstacles, or automatic shortening of a chain at an interior collision.
- Chain angle and length controls do not yet constitute a complete document style/stationery editor. They do not restyle existing molecules, rings or imported documents.
- Moruno labels whether a count is exact or a cap and explicitly includes attachment atoms.

The [full gap audit](feature-status.md) tracks the remaining template, chemistry, layout and application workflows.
