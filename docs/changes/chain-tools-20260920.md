# Chain tools: original checks (2026-09-20)

These checks describe the first chain tools build. The
[chain tools page](../chain-tools.md) describes how chains behave today.

## Desktop check

Desktop checks exercised fixed constraints, free drawing, exact-count insertion
and multi-segment snaking. An interchange fixture contains 16-, 6- and 20-carbon
chains with a 14.40 pt bond length and coordinate rounding below 0.01 pt.

1. Drew a chain in one drag; Check returned `C6H14`, six atoms and five bonds.
   Reading the saved native file confirmed each bond is 42 world units, or
   14.4 pt.
2. Reopened it, set Atoms to four and clicked an endpoint. The original endpoint
   was reused and the result was a continuous nine-carbon chain, `C9H20`.
3. Changed the preferred length to 20 pt, then used the JACS/ACS reset. The
   field returned to 14.4 without modifying existing objects.
4. Drew a snaking path with multiple pointer segments and captured the live
   **10 new C · 9 bonds** preview. Release produced those exact counts. One Undo
   removed the whole chain and Redo restored it.
5. Checked, fitted and saved the combined two-component drawing: `C19H42`, 19
   atoms and 17 bonds, all with 14.4 pt lengths.
6. In the packaged app, selected Times New Roman, 18 pt, bold, red text and
   20 pt bonds, then created a new document from the focused length field. The
   controls returned to Arial, 10 pt, normal black text and 14.4 pt bonds; the
   `X` drawing shortcut worked immediately afterward. Open, Save and Save As
   were also exercised from that field without inserting their command letters
   into its value. The file shortcuts are handled before widget text input.

## Tests

That build passed **83 Rust tests and 23 Python tests**, plus formatting and
Clippy. New tests covered independent constraints, exact sizes up to 512 atoms,
variable length, snaking and retracing, fast pointer events, preview/commit
identity, native/MOL/CDXML persistence, group growth, Undo/Redo and JACS
defaults after New. Local desktop evidence was kept under the ignored
`artifacts/chain-qa-20260920/` folder.
