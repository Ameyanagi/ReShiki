# Quantum chemistry file fixtures

Test inputs for the readers in `crates/io/src/qcread`. The cclib, pymatgen and VibeView files below are
unmodified copies of third-party files; some are renamed so that the program and
version are in the name. They are third-party material and keep their own
licences; they are not offered under ReShiki's `MIT OR Apache-2.0`. The files
in `reshiki/` were written for ReShiki and are under the project's licence.

## cclib (`cclib/`)

From [cclib](https://github.com/cclib/cclib) at commit
`1ed6fb7b7d0b95f2767c12071e22841e18bb845e`, BSD-3-Clause, copyright the cclib
development team. Licence text: [`licenses/cclib/LICENSE`](../../../licenses/cclib/LICENSE).
Source paths are relative to the repository root, so the original of
`cclib/g09_dvb_ir.fchk` is
<https://github.com/cclib/cclib/blob/1ed6fb7b7d0b95f2767c12071e22841e18bb845e/data/FChk/basicGaussian09/dvb_ir.fchk>.

| File                            | Source path                                     | Program          | SHA-256 (first 12) |
| ------------------------------- | ----------------------------------------------- | ---------------- | ------------------ |
| `uracil_two.xyz`                | `test/io/data/uracil_two.xyz`                   | XYZ, 2 frames    | `6affc838292f`     |
| `example_MEP_trj.xyz`           | `test/io/data/example_MEP_trj.xyz`              | ORCA XYZ path    | `ec4a10f2bfa5`     |
| `xtb661_dvb_xtbopt.xyz`         | `data/XTB/basicXTB6.6.1/dvb_opt/xtbopt.log`     | xtb 6.6.1        | `848d12c6cd1e`     |
| `xtb661_dvb_xtbtopo.mol`        | `data/XTB/basicXTB6.6.1/dvb_opt/xtbtopo.mol`    | xtb 6.6.1        | `e437e7ab8f21`     |
| `molden5.7_C_bigbasis.molden`   | `test/io/data/molden5.7_C_bigbasis.molden`      | Molden 5.7       | `750fa09016e6`     |
| `molden5.7_dvb_ir.molden`       | `test/io/data/molden5.7_dvb_ir.molden`          | Molden 5.7       | `9da024d6ed61`     |
| `molden5.7_dvb_un_sp.molden`    | `test/io/data/molden5.7_dvb_un_sp.molden`       | Molden 5.7       | `01d33cdf1fcc`     |
| `molden5.7_water_mp2.molden`    | `test/io/data/molden5.7_water_mp2.molden`       | Molden 5.7       | `1ace1b484f39`     |
| `g09_C_bigbasis.fchk`           | `data/FChk/basicGaussian09/C_bigbasis.fchk`     | Gaussian 09      | `5570be3fb24c`     |
| `g09_dvb_ir.fchk`               | `data/FChk/basicGaussian09/dvb_ir.fchk`         | Gaussian 09      | `2ffb736129cb`     |
| `g09_dvb_un_sp.fchk`            | `data/FChk/basicGaussian09/dvb_un_sp.fchk`      | Gaussian 09      | `27af65c5ff0b`     |
| `g09_Mo4OCl4-sp.fchk`           | `data/FChk/basicGaussian09/Mo4OCl4-sp.fchk`     | Gaussian 09      | `3e9d5f5cd960`     |
| `g09_water_mp2.fchk`            | `data/FChk/basicGaussian09/water_mp2.fchk`      | Gaussian 09      | `a61758cd1469`     |
| `g16_dvb_ir.fchk`               | `data/FChk/basicGaussian16/dvb_ir.fchk`         | Gaussian 16 A.03 | `351f6233e22a`     |
| `g16_dvb_gopt.out`              | `data/Gaussian/basicGaussian16/dvb_gopt.out`    | Gaussian 16      | `f4e280b32b0d`     |
| `g16_dvb_ir.out`                | `data/Gaussian/basicGaussian16/dvb_ir.out`      | Gaussian 16      | `bc1a21de15ad`     |
| `g16_water_mp2.log`             | `data/Gaussian/basicGaussian16/water_mp2.log`   | Gaussian 16      | `9a93eac63dba`     |
| `g16_dvb_gopt.gjf`              | `data/Gaussian/basicGaussian16/dvb_gopt.gjf`    | Gaussian input   | `c50445669b41`     |
| `g16_water_mp2.com`             | `data/Gaussian/basicGaussian16/water_mp2.com`   | Gaussian input   | `3610098d990c`     |
| `orca50_water_mp2.out`          | `data/ORCA/basicORCA5.0/water_mp2.out`          | ORCA 5.0         | `102d7006b82f`     |
| `orca60_dvb_gopt.out`           | `data/ORCA/basicORCA6.0/dvb_gopt.out`           | ORCA 6.0         | `3f1722c8807d`     |
| `orca60_dvb_ir.out`             | `data/ORCA/basicORCA6.0/dvb_ir.out`             | ORCA 6.0         | `fe363d324d00`     |
| `orca60_water_mp2.out`          | `data/ORCA/basicORCA6.0/water_mp2.out`          | ORCA 6.0         | `7c27896ef6f7`     |
| `orca61_dvb_coupling_nmr.out`   | `data/ORCA/basicORCA6.1/dvb_coupling_nmr.out`   | ORCA 6.1         | `988d0549c7ec`     |
| `orca60_dvb_gopt.inp`           | `data/ORCA/basicORCA6.0/dvb_gopt.inp`           | ORCA input       | `99f852a614e2`     |
| `orca60_water_mp2.inp`          | `data/ORCA/basicORCA6.0/water_mp2.inp`          | ORCA input       | `7d2ca680f644`     |
| `orca60_dvb_scan_unrelaxed.inp` | `data/ORCA/basicORCA6.0/dvb_scan_unrelaxed.inp` | ORCA input       | `0a9378e8af32`     |
| `gamess_C_bigbasis.inp`         | `data/GAMESS/basicGAMESS-US2018/C_bigbasis.inp` | GAMESS input     | `129c1e1bce85`     |
| `gamess_dvb_gopt_a.inp`         | `data/GAMESS/basicGAMESS-US2018/dvb_gopt_a.inp` | GAMESS input     | `02234bf42a34`     |
| `gamess_nh3_ts_ir.inp`          | `data/GAMESS/basicGAMESS-US2018/nh3_ts_ir.inp`  | GAMESS input     | `f759f4687a5b`     |

The Molden 5.7 files were written from GAMESS-US runs (cclib compares them with
GAMESS output), not from the Gaussian runs of the fchk files of the same name.
They are not same-calculation pairs: the tests use them for the Molden
Cartesian normalization (orthonormal orbitals) and for cross-program checks of
counts and energies only.

## pymatgen-test-files (`pymatgen/`)

From [materialsproject/pymatgen-test-files](https://github.com/materialsproject/pymatgen-test-files)
at commit `85005108e99e40cbd59e78f067459b0ca664efc6`, directory `io/gaussian/`,
MIT, copyright Materialyze.AI. Licence text:
[`licenses/pymatgen-test-files/LICENSE`](../../../licenses/pymatgen-test-files/LICENSE).

| File                         | Program                              | SHA-256 (first 12) |
| ---------------------------- | ------------------------------------ | ------------------ |
| `CH2O_CO2.log`               | Gaussian 09 D.01, two jobs           | `537e5f103b64`     |
| `H2O_gau_vib.out`            | Gaussian 09 D.01, opt then freq      | `b7e841b77d31`     |
| `methane.log`                | Gaussian 03 D.01, optimization       | `a5dd069ddefe`     |
| `so2_scan.log`               | Gaussian 09 D.01, rigid scan         | `16ea5720d1c8`     |
| `MethylPyrrolidine_drawn.gjf` | Gaussian input, Z-matrix            | `5c5e587e5997`     |
| `l-cysteine.inp`             | Gaussian input, Z-matrix             | `fbbeadd5c75f`     |
| `C60_gau.com`                | Gaussian input, Cartesian            | `aa2c7e8a010d`     |

## ORCA (`orca/`)

From [ClevinLiju/VibeView](https://github.com/ClevinLiju/VibeView) at commit
`1ff906a7e74e3f83b79370f0f80a07acf7028fc9`, MIT, copyright Clevin Liju and
VibeView contributors. The licence text is in
[`licenses/vibeview/LICENSE`](../../../licenses/vibeview/LICENSE).
These are unmodified ORCA 6.1.1 `orca_2mkl` Molden exports under local filenames.
Source paths are relative to the VibeView repository root.

| File                        | Source path                                               | SHA-256 (first 12) |
| --------------------------- | --------------------------------------------------------- | ----------------- |
| `orca611_water_tzvp.molden`  | `Sources/MolChecks/TestData/analysis/water_tzvp.molden`     | `9f17122eaf0b`    |
| `orca611_h2co_tddft.molden`  | `Sources/MolChecks/TestData/io/orca/h2co_tddft.molden.input` | `c71c2fd033e3`    |

## PySCF (`pyscf/`)

`scripts/qc_orbital_reference.py` generates these Molden files with PySCF 2.15.0,
an Apache-2.0 tool. The generated data is offered under ReShiki's MIT OR
Apache-2.0 licence. The two mixed water exports transform selected angular
momenta of the pure cc-pVQZ calculation to Cartesian shells.

## Signed orbital references (`reference/`)

The same script generates signed MO values at 32 fixed points in bohr, ordered
by the energies printed in each source file. The generated values are offered
under ReShiki's MIT OR Apache-2.0 licence.

| File                       | Reference route                                                                              |
| -------------------------- | -------------------------------------------------------------------------------------------- |
| `ch2o_ccpvtz_pure.txt`     | PySCF RHF/cc-pVTZ pure AO evaluation times the MO coefficients.                              |
| `h2o_ccpvqz_pure.txt`      | PySCF RHF/cc-pVQZ pure AO evaluation times the MO coefficients.                              |
| `h2o_ccpvqz_cart.txt`      | PySCF RHF/cc-pVQZ Cartesian AO evaluation times the MO coefficients.                         |
| `ch3nh2_631gs_cart.txt`    | PySCF RHF/6-31G* Cartesian AO evaluation times the MO coefficients.                          |
| `tih2f2_def2tzvp_pure.txt` | PySCF RHF/def2-TZVP pure AO evaluation times the MO coefficients.                            |
| `h2o_ccpvqz_mixed_a.txt`   | Pure water reference, cross-checked by transforming only f shells to Cartesian.              |
| `h2o_ccpvqz_mixed_b.txt`   | Pure water reference, cross-checked by transforming d and g shells to Cartesian.             |
| `g09_Mo4OCl4-sp.txt`       | PySCF pure AO evaluation with Gaussian fchk coefficients mapped by the G09/G16 overlap test. |
| `orca611_water_tzvp.txt`   | PySCF pure AO evaluation using the ORCA convention established by implied overlap.             |
| `orca611_h2co_tddft.txt`   | PySCF pure AO evaluation using the ORCA convention established by implied overlap.             |

## ReShiki (`reshiki/`)

Written for ReShiki under the project's MIT OR Apache-2.0 licence.

- `ions_3d.sdf` contains V2000 water and ammonium and V3000 deuteroxide, with
  every atom given 3D coordinates.

## Licence rule for new fixtures

Use cclib `data/` and `test/io/data/` (BSD-3-Clause), pymatgen-test-files
`io/gaussian` (MIT), VibeView (MIT), or other MIT, BSD or Apache-licensed
sources. Record the source path, commit, licence and hash of each copied
file here, and retain the upstream licence text under `licenses/`. Never copy
from cclib-data (no licence), ASE or IOData (copyleft).
