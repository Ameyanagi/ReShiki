---
section: Changes
issues: [1203]
credit: [@ssiddhantsharma]
---

- Write predicted NMR shifts onto the atoms from scripts: `with_labels(shifts, format="both")` in [Python](python.md#label-the-atoms), `convert --nucleus 13C --labels both` on the [command line](agent-cli.md#nmr-labels) and the `label_atoms` [MCP tool](agent-api.md#tools). Labels read `#3`, `1.28 ppm` or `#3 · 1.28 ppm`, as in **Show labels**, with 2 decimal places for ¹H and 1 for ¹³C unless you choose 0 to 4. They are stored as atom numbers, so `.rsk`, SVG, PNG, PDF, CDXML and CDX files keep them, and are placed clear of bonds and other labels, with a thin leader line when one has to sit away from its atom. `to_rdkit()` and `from_rdkit()` carry atom numbers as RDKit's `atomNote`. **Show labels** in the app stays an on-screen overlay.
