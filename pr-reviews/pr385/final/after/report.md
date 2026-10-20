# ReShiki → ChemDraw 26 → ReShiki round-trip check

**Result: both requested bond-attachment settings PASS on the verified requested debug binary.**

| Prefix | Setting sent | Exact value read back in step 6 | Result |
| --- | --- | --- | --- |
| L | Flush left | Flush left | PASS |
| C | Centered | Centered | PASS |

For both final read-backs, the pasted MeO atom was selected and the Atom labels & numbering pane showed **Selected atoms**, **1 atoms in scope**, and the value above under **Bond attachment (text labels)**. The label remained editable atom text; a contracted-group alignment control was not needed.

## Final evidence

### Flush left (L)

- Source setting screenshot: [L-1-reshiki-set.png](L-1-reshiki-set.png)
- ReShiki outgoing clipboard: [L-2-reshiki-clipboard/types.json](L-2-reshiki-clipboard/types.json)
- ChemDraw screenshot: [L-3-chemdraw.png](L-3-chemdraw.png)
- ChemDraw outgoing clipboard: [L-4-chemdraw-clipboard/types.json](L-4-chemdraw-clipboard/types.json)
- ReShiki read-back screenshot: [L-5-reshiki-after.png](L-5-reshiki-after.png)

### Centered (C)

- Source setting screenshot: [C-1-reshiki-set.png](C-1-reshiki-set.png)
- ReShiki outgoing clipboard: [C-2-reshiki-clipboard/types.json](C-2-reshiki-clipboard/types.json)
- ChemDraw screenshot: [C-3-chemdraw.png](C-3-chemdraw.png)
- ChemDraw outgoing clipboard: [C-4-chemdraw-clipboard/types.json](C-4-chemdraw-clipboard/types.json)
- ReShiki read-back screenshot: [C-5-reshiki-after.png](C-5-reshiki-after.png)

Each definitive case used a fresh ReShiki source tab, imported CC through the Import UI, applied cleanup, and displayed the source at 250%. The left carbon was selected, Enter opened the label editor, MeO was entered, and Enter applied it. The whole molecule was copied with Cmd+A/Cmd+C. ChemDraw used a new document, and its Change Settings dialog was brought forward through Window and answered **As Copied**. The ChemDraw view was magnified three times for the screenshot, then Cmd+A/Cmd+C copied the result into another fresh ReShiki tab. Return tabs displayed at 100%.

All four clipboard captures used the requested Swift helper. ReShiki captures contain seven representations, including native ReShiki data and three CDX UTIs (434 bytes each). ChemDraw captures contain five representations, including CDX (L: 1,273 bytes; C: 1,278 bytes), and contain no native ReShiki representation. All 24 payload files exist and match their metadata byte counts. A separate read-only reviewer confirmed the four setting screenshots and capture-file sizes. The screenshot tool returned JPEG-encoded images; the six final screenshots were converted to actual PNG without cropping or annotation.

## Build targeting and deviations

The initial name-based computer-use binding resolved to the installed ReShiki copy instead of the separately launched debug process. That initial run is **not valid evidence for the requested binary**. Its artifacts are preserved only in `unverified-name-resolved-run/`; they showed Flush right after both cases and are superseded by the final top-level artifacts above. The initial Centered label entry also needed one undo/re-entry after producing O; this occurred only in that superseded run.

To make the requested build unambiguous, a temporary uniquely named macOS app wrapper was created inside this evidence directory. Its executable was a **hard link to the exact requested binary**, not a rebuild or modified copy:

`/Users/shiranui/dev/ReShiki/.claude/worktrees/agent-a826e1a78594af95e/target/debug/reshiki`

Source and wrapper had the same inode, 13539433. SHA-256:

`5d9345f0dfafea352fce20461a57e55ff3270d7d8018d5f719a8e134d7cea7d4`

The wrapper executable was launched from a shell in the background and computer use was bound by its exact app path, with app identity **ReShiki RoundTrip Check**. PID 43389 was confirmed to be the only running ReShiki process for both definitive cases. [test-binary-identity.json](test-binary-identity.json) records the source, temporary wrapper path, inode and hash. The source hash was checked again after testing. The temporary wrapper was removed after shutdown. The initial direct launch and resolver-launched temporary instances were also stopped; no repository file contents were edited.

Other observations:

- Both definitive ChemDraw pastes retained the visible text **MeO** and displayed a red chemical-warning outline. In the Flush left ChemDraw screenshot, no bond stroke is visible outside the label; in the Centered screenshot a short stroke appears to its right. The copied-back ReShiki drawings contain the atom and bond. These visual observations do not change the requested pass criterion, which compares the attachment setting.
- In L, the separate **Hydrogen position** control changed from Automatic to Left. In C it remained Automatic. Bond attachment itself read back correctly in both cases.
- ReShiki displayed a Recovery draft banner. It was not restored; all test drawings used fresh tabs.
- ChemDraw sometimes changed its temporary document title to “ChemDraw Document” or “ChemDraw Unsaved Document.” No Save action was taken.
- Computer use timed out while reading ChemDraw after its temporary document was discarded and no key document was visible. An additional empty temporary document was used to inspect the Window menu, then closed untouched. This inventory check was performed after each overall run.

## Cleanup

The definitive ChemDraw documents began as **Untitled Document-32** and **Untitled Document-33**. Both were closed with Cmd+W and discarded using **Delete** in the save sheet. The superseded run's documents 29 and 30 were also discarded. Empty inventory-check documents 31 and 34 were closed without saving.

The final Window menu contained the same three pre-existing documents, plus only the temporary empty inventory-check document before it was closed:

- Hotkeys Cheat Sheet for macOS.cdxml
- Shortcuts Cheat Sheet for macOS.cdxml
- Untitled ACS Document 1996-1

Their contents were not edited and they were not closed. The recorded menu is [chemdraw-final-window-inventory.txt](chemdraw-final-window-inventory.txt).

The verified ReShiki process was quit with Cmd+Q; it exited with code 0 and PID 43389 was confirmed absent. No save prompt appeared and no test drawing was saved. No other application was quit and no system settings were changed.
