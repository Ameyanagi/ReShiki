# Crystal structure fixtures

Hand-written for ReShiki's crystal readers (#417). No file is copied from a
database; each is typed from published cell and atom parameters, with the
symmetry operators of the International Tables for Crystallography. They are
offered under the project's license (`MIT OR Apache-2.0`).

| File             | Structure                 | Parameters                                                                     |
| ---------------- | ------------------------- | ------------------------------------------------------------------------------ |
| `nacl.cif`       | Rock salt NaCl, Fm-3m     | a = 5.6402 Å; Na 4a, Cl 4b; all 192 general-position operators                 |
| `co2.cif`        | Dry ice CO₂, Pa-3         | a = 5.624 Å; C 4a, O 8c with x = 0.1185; illustrative displacement parameters |
| `urea.pdb`       | Urea, P-42₁m, one molecule | a = 5.565 Å, c = 4.684 Å; atom positions as fractional coordinates × cell     |
| `rutile.vasp`    | Rutile TiO₂, P4₂/mnm      | a = 4.5937 Å, c = 2.9587 Å, u = 0.3048; POSCAR, Direct                         |
| `water.xyz`      | Water                     | Plain XYZ, no cell                                                             |
| `silicon.extxyz` | Silicon (diamond)         | a = 5.431 Å, primitive fcc cell; extended XYZ with an extra `forces` column    |

`nacl.cif` also carries a bibliography text field, a reflection loop and a
second data block, and `silicon.extxyz` extra comment keys, each holding a
`DROPPED-…-MARKER`: the tests check that a saved drawing keeps none of them.
