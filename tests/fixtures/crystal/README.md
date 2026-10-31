# Crystal structure fixtures

Except for the two COD entries below, hand-written for ReShiki's crystal
readers (#417). No other file is copied from a database; each is typed from
published cell and atom parameters, with the
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

`cod-7033930.cif` is an unmodified Crystallography Open Database entry
[7033930](https://www.crystallography.net/cod/7033930.cif), revision 180045
(2016-03-25), downloaded 2026-10-25. It is third-party data, dedicated to the
public domain under [CC0](https://www.crystallography.net/cod/); the original
public-domain notice is preserved in the file. The structure is from Yasuhiro
Miyamoto and Satoshi Takamizawa, “Deformation twinning of ferrocene crystals
assisted by the rotational mobility of cyclopentadienyl rings,” *Dalton
Transactions* **44** (2015), 5688–5691,
[doi:10.1039/C4DT03922J](https://doi.org/10.1039/C4DT03922J). The fixture tests
the saved import note for ten excluded alternative site records.

`cod-1516479.cif` is an unmodified Crystallography Open Database entry
[1516479](https://www.crystallography.net/cod/1516479.cif), revision 274633
(2022-04-22), downloaded 2026-10-31. It is third-party data, dedicated to the
public domain under [CC0](https://www.crystallography.net/cod/); the original
public-domain notice is preserved in the file. The aspirin structure is from
Harrison, Ibberson, Robb, Whittaker, Wilson and Youngson, “In situ neutron
diffraction studies of single crystals and powders during microwave
irradiation,” *Faraday Discussions* **122** (2003), 363–379,
[doi:10.1039/b203379h](https://doi.org/10.1039/b203379h). The fixture tests the
opening view's atom-label clearance and its import note when labels overlap.
