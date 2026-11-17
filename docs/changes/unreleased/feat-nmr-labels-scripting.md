---
section: Changes
issues: [1203]
credit: [@ssiddhantsharma]
---

- Write predicted NMR shifts onto the atoms from scripts: `with_labels(shifts, format="both")` in [Python](python.md#label-the-atoms), `convert --nucleus 13C --labels both` on the [command line](agent-cli.md#nmr-labels) and the `label_atoms` [MCP tool](agent-api.md#tools). Labels read `#3`, `1.280 ppm` or `#3 · 1.280 ppm`, as in **Show labels**, and are stored as atom numbers, so `.rsk`, SVG, PNG, PDF, CDXML and CDX files keep them. **Show labels** in the app stays an on-screen overlay.
