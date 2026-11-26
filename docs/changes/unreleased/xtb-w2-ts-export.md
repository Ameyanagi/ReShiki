---
section: Changes
issues: []
---

- `reshiki --cli export-input` writes transition-state path inputs: `--reactant`
  and `--product` for Gaussian QST2/QST3 and ORCA NEB-TS, and `--hessian` reads
  an ORCA `.hess` file (such as the one `reshiki --cli ts` writes) so ORCA OptTS
  inputs start from that Hessian with `InHess Read` and IRC inputs use
  `InitHess read` with `Hess_Filename`. See
  [Agent CLI](agent-cli.md).
