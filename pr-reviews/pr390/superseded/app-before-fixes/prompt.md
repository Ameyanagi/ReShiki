You are running local UI verification for Claude on macOS. Use computer use (screenshots, mouse, keyboard).

Repository (branch feat/3d-models-1a, already built): /Users/shiranui/dev/ReShiki/.claude/worktrees/wf_9a402735-96b-2
App binary: /Users/shiranui/dev/ReShiki/.claude/worktrees/wf_9a402735-96b-2/target/debug/reshiki
Artifact directory (save ALL screenshots and the report here): /private/tmp/claude-502/-Users-shiranui-dev-ReShiki/dd6a248f-7145-4e1e-bc5b-a50dfb88eed0/scratchpad/ev386-1a/app

Do NOT rebuild. Launch the binary directly (e.g. run it in the background from a shell: `/Users/.../target/debug/reshiki &`). It opens the ReShiki molecular drawing window. If another ReShiki window from a different build is already open, leave it alone and work only in the window you launched.

Feature under test: the Properties (inspector) panel has a new "Structure" section with a "Show as" control offering 2D / 3D / Both. It appears when exactly one molecule is selected. Choosing 3D generates an MMFF94s conformer in a background worker (status bar says "Generating 3D model · MMFF94s") and then shows the molecule as a ball & stick model (shaded spheres and bonds) inside a frame. Both shows the 2D drawing and the 3D model together. 2D goes back to the plain drawing. Each switch is one Undo step (Cmd+Z).

Flow:
1. Launch the app. Screenshot: 01-launched.png.
2. Insert caffeine: open the structure import panel (look for an Import / Insert structure control in the toolbar or sidebar; it has a text box with placeholder "Paste SMILES, reaction SMILES, InChI, MOL, RXN or CDXML"). Type exactly: CN1C=NC2=C1C(=O)N(C(=O)N2C)C  then press Cmd+Enter (or click Insert). Expect the caffeine structure on the canvas, selected. Screenshot: 02-caffeine-inserted.png.
3. Make sure caffeine (the whole molecule) is selected (Select tool; Cmd+A selects all). Find the Structure section in the Properties panel on the right (scroll the panel if needed). Expect "2D" active. Screenshot including canvas and the Structure section: 03-structure-section-2d.png.
4. Click "3D". Screenshot right away if you can catch the "Generating" state: 04a-generating.png (optional). Wait until the status bar no longer says Generating (a few seconds). Expect: the molecule drawn as a 3D ball & stick model in a frame, 2D drawing hidden, "3D" active in the Structure section. Screenshot: 04-3d.png. Also zoom into the model if it is small (do not change the drawing) and take 04b-3d-closeup.png.
5. Click "Both". Expect: 2D drawing and 3D model both visible; "Both" active. Screenshot: 05-both.png.
6. Click "2D". Expect: plain 2D drawing only; "2D" active. Screenshot: 06-2d.png.
7. Press Cmd+Z once (Undo). Expect: back to Both (the previous state). Screenshot: 07-undo.png.
For every screenshot also note the status bar text and the window title (unsaved marker).

Constraints:
- Stay read-only toward the user's environment: do not close or quit other applications, do not change system settings, do not edit any files in the repository, do not save the drawing anywhere except optionally into the artifact directory.
- When done, quit only the ReShiki instance you launched (choose "Don't Save" if asked).

Write report.md into the artifact directory: for each step, what you did, observed vs expected (including status bar text), the screenshot filename, and a pass/fail verdict. End with anything you could not verify and why, and any visual oddities you noticed (overlaps, clipping, wrong colours, frame placement, labels left over on hidden atoms, etc.).
