# Issue #507 native Fit evidence

Ten full native application screenshots cover all five reported inputs before and after the fix. The [capture receipt](receipt.json) records their source and hashes. All screenshots are unchanged 2560×1696 JPEG bytes returned by `cua_repl`. The source executables are the exact retained release-timing pair: official 0.13.0 (`3d80590a`, SHA-256 `4f54b01861a4e399e5f17adeb05372d39241a4b3227eba55d889b3d2d7d88739`) and accepted unsigned 0.13.1 manual package (`3d3eed65`, SHA-256 `1bd5e0c40a5a005373493226ec4f78a31fdf615254437901c845abc12a9a1b23`). Both binary hashes remained unchanged. No rebuild or timing rerun was performed.

The native full-window size, default light theme, white canvas, JACS / ACS style, Publication display and expanded Whole drawing Properties panel match. Native Fit determines each geometry's zoom; this is not a comparison at equal magnification. Minor accepted version UI differences remain visible.

| Input               | Formula; atoms / bonds         | Before                                               | After                                              |
| ------------------- | ------------------------------ | ---------------------------------------------------- | -------------------------------------------------- |
| Polystyrene ×30     | C240H242; 240 / 269            | [Native Fit (36%)](polystyrene30-before-fit.jpg)     | [Native Fit (39%)](polystyrene30-after-fit.jpg)    |
| Polystyrene ×75     | C600H602; 600 / 674            | [Native Fit (15%)](polystyrene75-before-fit.jpg)     | [Native Fit (16%)](polystyrene75-after-fit.jpg)    |
| Polystyrene ×100    | C800H802; 800 / 899            | [Native Fit (11%)](polystyrene100-before-fit.jpg)    | [Native Fit (12%)](polystyrene100-after-fit.jpg)   |
| Alanine ×200        | C600H1002N200O201; 1001 / 1000 | [Native Fit (78%)](alanine200-before-fit.jpg)        | [Native Fit (4%)](alanine200-after-fit.jpg)        |
| Poly-L-alanine ×200 | C600H1002N200O201; 1001 / 1000 | [Native Fit (78%)](stereo-alanine200-before-fit.jpg) | [Native Fit (4%)](stereo-alanine200-after-fit.jpg) |

All five fixed screenshots and the three baseline polystyrene screenshots used fresh native Import → Structure, exact SMILES paste, Insert, deselection and Fit. The baseline alanine import became unobservable after insertion; its retained main-thread diagnostic points to selection/label work, without establishing an InChI-specific cause. The two baseline peptide screenshots therefore use native Open of preserved regression RSKs with no selection, followed by the native Cmd+/ Fit command. They establish baseline native appearance, not successful fresh dialog interaction. All atoms, bond endpoints/orders and coordinates independently match the retained exact0.13.0 CLI MOL output (maximum coordinate difference 0.000036 native units after scale 28/y inversion). The stereochemical RSK retains 200 centers and 200 wedge-or-hash bonds. Those are artifact checks, not a visual count.

At Fit, baseline polystyrene shows a crowded ladder region and the fixed drawing shows separated alternating ring rows. Baseline peptides show a dense cluster with superposed labels and wedges. Fixed peptides span an open horizontal chain, but at 4% their individual labels/wedges are too small to judge. Existing application-renderer closeups and chemistry/geometry checks provide the detail evidence; these native screenshots supplement them.

The complete working receipt, exact input identities, native AX state snapshots, preserved baseline RSKs and diagnostic sample are under `evidence/release-0.13.1/completion-audit/507-native-fit`. Task-owned app sessions ended; personal app settings and files were not changed. This evidence makes no performance claim.
