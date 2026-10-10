# Assigned experimental NMR fixtures

The `.sd` files are verbatim records from the pinned nmrshiftdb2 2026-03-15
`nmrshiftdb2withsignals.sd` export, SHA-256
`0e86688360e23c88ccf0eb82a1251315fa57ec6f5b376dc8886f3311793a0afe`.
Separately licensed under the [nmrshiftdb2 Database License](../../../licenses/nmrshiftdb2/DATABASE-LICENSE.txt).
Copyright/database rights remain with the nmrshiftdb2 contributors.
Contains information from [nmrshiftdb2](https://nmrshiftdb.nmr.uni-koeln.de),
which is made available here under that license.

- `explicit-assigned-h.sd`: molecule 2459. Its measured ¹H spectrum 1 assigns
  some shifts to explicit H atoms and others to parent heavy atoms, including
  multiple non-equivalent shifts on a parent. Retained to verify mapping/group
  ambiguity behavior; values must not be turned into arbitrary individual-H
  assignments.
- `mixed-measured-calculated.sd`: molecule 2243. Indexed `Program` metadata
  marks carbon spectra 1/2 as computed, while measured spectra 0/3 and proton
  spectrum 4 have measured-condition metadata. Retained to verify that computed
  spectra are excluded, not silently mixed with observed training shifts.

Full provenance, pinned source download and transformation are in
[data/nmr/README.md](../../../data/nmr/README.md).

`ethyl-acetate.rsk` is an original ReShiki drawing under MIT OR Apache-2.0,
for native GUI review of the fully supported three-H-group/four-carbon
acceptance example. It contains no experimental shifts.
