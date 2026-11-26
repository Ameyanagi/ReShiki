---
section: Changes
issues: []
---

- Transition-state search from the command line: `reshiki --cli ts` takes two
  atom-mapped XYZ endpoints (or one endpoint and the bonds that form and break),
  optimizes them with GFN2-xTB, runs IDPP interpolation, a climbing-image nudged
  elastic band and a P-RFO saddle refinement, checks for exactly one imaginary
  frequency and follows the IRC both ways to confirm the saddle connects reactant
  and product. It writes the TS, endpoints, path, IRC and an ORCA `.hess` file.
  Desktop builds with xTB only (macOS and Linux). See [Agent CLI](agent-cli.md).
