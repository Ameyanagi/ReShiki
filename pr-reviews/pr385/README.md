# Issue #378 evidence: label alignment through ChemDraw

Branch `fix/label-justification-chemdraw` (e7d1a2fd, 6562d2bf). ChemDraw 26.1.0.6801 on macOS, driven by Codex computer use (gpt-6-astra). All tag tables below were decoded from the saved files or captured clipboard CDX by Claude with ReShiki's own CDX codec, not taken from Codex's prose.

## a/: what ChemDraw saves (MeO on ethane, Text menu)

`meo-*.cdxml/.cdx` were drawn and saved in ChemDraw. Tags are listed in `a/tags.txt`. The CDXML files are committed as `tests/fixtures/chemdraw-attachments/label-justification-*.cdxml`.

| Text menu | node `LabelDisplay` (0x0401) | t `LabelJustification` (0x0823) | t `Justification` (0x0701) | t `LabelAlignment` (0x0705) |
|---|---|---|---|---|
| Flush Left | Left | Left | (absent = Left) | Right |
| Centered | Center | Center | Center | Right |
| Flush Right | Right | Right | Right | Right |
| Automatic | (absent) | Right | Right | Right |

`LabelAlignment` is Right for every choice, so it is not the flush choice. The choice lives in the node's `LabelDisplay`. ChemDraw turns a flush-right MeO into a methoxy `Fragment`.

## b/: which tags survive a paste (MeO)

`b/set_clipboard.swift` puts each candidate on the pasteboard under ReShiki's CDX and CDXML types. Codex pasted it into a new ChemDraw document, then pressed Cmd+A and Cmd+C, and `scripts/chemdraw_capture_clipboard.swift` captured the result. The route is paste, not open. The files sent are in `b/candidates/` (`b/tags-sent.txt`). The data ChemDraw sent back is in `b/back/*/item-0-1.bin` (`b/tags-returned.txt`).

| Candidate | Tags sent | Returned for Left / Center / Right |
|---|---|---|
| A (main) | 0x0823 (node `LabelDisplay=Auto`) | Right / Right / Right |
| B | 0x0823 + 0x0701 | Right / Right / Right |
| C | 0x0823 + 0x0701 + 0x0705=Right | Right / Right / Right |
| D (fix) | node 0x0401 + 0x0823 | **Left / Center / Right** |
| E | node 0x0401 + 0x0823 + 0x0701 | Left / Center / Right |

The D and E Right cases come back as a methoxy Fragment (SMILES `COC`).

## c/: the same check for groups (Boc)

G0 is main's group export, with no node `LabelDisplay`. In `c/tags-returned.txt`, every G0 alignment came back as Left (Automatic). G1-401 adds the node's `LabelDisplay`, and Left, Center, Right and Above all came back unchanged. Note: after the run, the generator overwrote `c/candidates/G0-left.cdxml` and `G1-401-left.cdxml`. They were rebuilt to match what was pasted, and `G0-left.cdx` was removed.

## final/: real app, ReShiki → ChemDraw → ReShiki (paste route, as in the issue)

- `final/after/`: branch build `target/debug/reshiki` at e7d1a2fd, launched through a hard-linked wrapper app. **Flush left → Flush left, Centered → Centered** (`L-5-reshiki-after.png`, `C-5-reshiki-after.png`). The tags ReShiki sent and ChemDraw returned are in `final/after/tags.txt`. In this run, Hydrogen position read Left after the Flush left round trip. 6562d2bf fixes that, and `tests/atom_labels.rs` checks it. The app was not run again after that commit.
- `final/before-installed-nightly/`: the first attempt bound to the installed ReShiki nightly instead of the debug build. The issue's bug reproduces there: Flush left and Centered both come back as **Flush right**.
- `final/before/`: the `origin/main` debug build. The ReShiki side completed, and `final/before/tags.txt` shows main sends `LabelDisplay="Auto"` with `LabelJustification` only (the same as candidate A). The ChemDraw leg was blocked because ChemDraw stopped responding to computer use (about 97% CPU), so there is no main-build read-back.
