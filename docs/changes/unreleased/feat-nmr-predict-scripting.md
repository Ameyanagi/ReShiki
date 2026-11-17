---
section: Changes
issues: [1203]
credit: [@ssiddhantsharma]
---

- Predict ¹H or ¹³C NMR shifts from scripts: `Molecule.predict_nmr("13C")` in
  [Python](python.md#predict-nmr), `reshiki --cli predict-nmr` on the
  [command line](agent-cli.md#predict-nmr) and the `predict_nmr`
  [MCP tool](agent-api.md#tools). Each returns the rows the **Predict NMR…**
  palette lists, with the broad-match flag, and the method, data version and
  data credit that **Export…** writes. It runs offline.
