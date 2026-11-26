# References for xTB features

The methods and algorithms behind ReShiki's xTB calculations, optimizer, conformer search,
vibrations, orbital surfaces and transition-state search, with what each is used for and
where it lives in the code. Code comments name a source by a short author–year mention and
its key in brackets, for example `(Wang & Song 2016) [Wang2016TRIC]` (written `\[Key\]` in
Rust doc comments so rustdoc does not treat it as a link); the key is the BibTeX key in
[xtb.bib](xtb.bib).

Every entry with a DOI was checked against its Crossref record on 26 November 2026
(authors, title, journal, volume, first page or article number, year); RDKit's DOI is
registered with DataCite and was checked there. Books without a Crossref record for the
cited edition were checked by ISBN on Open Library, the thesis on HAL; software and format
specifications are cited by URL. [Verification notes](#verification-notes) lists what could not
be checked.

## What to cite

For every calculation ReShiki runs, cite ReShiki (`ReShiki`), axtb (`Axtb`) and the method:
GFN2-xTB is `Bannwarth2019GFN2`, GFN-FF is `Spicher2020GFNFF` (plus `Rose2024GFNFFLnAn` for
lanthanides and actinides) and g-xTB is `Froitzheim2025gxTB`. `Bannwarth2021Xtb` covers the
xtb program whose conventions axtb reproduces. For results opened from another program's
files, cite ReShiki and that program and method instead of axtb. Then add:

| Feature                                                            | Also cite                                                                                                                                                                                   |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Implicit solvent (ALPB or GBSA)                                    | Ehlert2021ALPB                                                                                                                                                                              |
| Geometry optimization, default (TRIC with rational-function steps) | Wang2016TRIC, Baker1996DLC, Lindh1995Hessian, Banerjee1985PRFO                                                                                                                              |
| Geometry optimization, ANC                                         | Grimme2017ANCopt, Lindh1995Hessian (with the default model Hessian), Banerjee1985PRFO                                                                                                       |
| Geometry optimization, L-BFGS                                      | Liu1989LBFGS, Armijo1966LineSearch                                                                                                                                                          |
| Conformer search                                                   | Riniker2015ETKDG, Wang2020ETKDGv3, Butina1999Clustering, and what minimized the starts: Halgren1999MMFF94s (default), Halgren1996MMFF, Rappe1992UFF, or the GFN-FF or GFN2-xTB method paper |
| Frequencies and thermochemistry                                    | Grimme2012QRRHO, FernandezRamos2007Symmetry                                                                                                                                                 |
| IR spectrum                                                        | nothing beyond the method                                                                                                                                                                   |
| Orbital energies and coefficients                                  | nothing beyond the method                                                                                                                                                                   |
| Orbital surfaces in a figure                                       | Lorensen1987MarchingCubes (and Doi & Koide 1991, see the verification notes), Taubin1995Smoothing, Garland1997Simplification; Schneider1990CurveFit for vector outlines                     |
| Transition-state search (GFN2-xTB only)                            | Smidstrup2014IDPP, Henkelman2000CINEB, Henkelman2000Tangent, Bitzek2006FIRE, Banerjee1985PRFO, Baker1986PRFO, Bofill1994Update, PageMcIver1988IRC                                           |
| Results read from another program's files                          | ReShiki and that program (for ORCA 6, Neese2025ORCA6)                                                                                                                                       |

## Calculation methods

All three methods are computed by the axtb fork, linked into the desktop app under the `xtb`
feature and run in the `reshiki --xtb-worker` child process (`crates/xtb-worker`). Method
labels and capabilities: `crates/qcdata/src/capabilities.rs`.

- **Bannwarth2019GFN2**: Bannwarth, Ehlert & Grimme, J. Chem. Theory Comput. 15, 1652
  (2019), doi:10.1021/acs.jctc.8b01176. GFN2-xTB, the default method and the only one for
  transition states.
- **Spicher2020GFNFF**: Spicher & Grimme, Angew. Chem. Int. Ed. 59, 15665 (2020),
  doi:10.1002/anie.202004239. GFN-FF, used alone and for minimizing conformer starts.
- **Rose2024GFNFFLnAn**: Rose et al., Inorg. Chem. 63, 19364 (2024),
  doi:10.1021/acs.inorgchem.4c03215. GFN-FF's lanthanide and actinide extension, included in
  axtb's GFN-FF.
- **Froitzheim2025gxTB**: Froitzheim, Müller, Hansen & Grimme, ChemRxiv preprint (2025),
  doi:10.26434/chemrxiv-2025-bjxvt. g-xTB; ReShiki ships g-xTB 2.0.1 only
  (<https://github.com/grimme-lab/g-xtb>).
- **Bannwarth2021Xtb**: Bannwarth et al., WIREs Comput. Mol. Sci. 11, e1493 (2021),
  doi:10.1002/wcms.1493. The xtb program and the GFN family; the reference for axtb's
  "xtb 6.8 compatible" conventions.
- **Caldeweyher2017D4**: Caldeweyher, Bannwarth & Grimme, J. Chem. Phys. 147, 034112 (2017),
  doi:10.1063/1.4993215, and **Caldeweyher2019D4**: Caldeweyher et al., J. Chem. Phys. 150,
  154122 (2019), doi:10.1063/1.5090222. The D4 dispersion model inside GFN2-xTB; GFN-FF
  uses a simplified dispersion with D4 reference polarizabilities (see Spicher2020GFNFF).
- **Johnson1988Broyden**: Johnson, Phys. Rev. B 38, 12807 (1988),
  doi:10.1103/PhysRevB.38.12807, and **Pulay1980DIIS**: Pulay, Chem. Phys. Lett. 73, 393
  (1980), doi:10.1016/0009-2614(80)80396-4. Modified Broyden and DIIS mixing in axtb's
  GFN2-xTB SCF.

## Solvation

Implicit solvent runs inside axtb; ReShiki selects the model and solvent (`SolventModel` in
`crates/qcdata/src/result.rs`, `crates/xtb-worker/src/protocol.rs`). g-xTB in solvent adds
GFN2-xTB's solvation energy.

- **Ehlert2021ALPB**: Ehlert et al., J. Chem. Theory Comput. 17, 4250 (2021),
  doi:10.1021/acs.jctc.1c00471. ALPB and GBSA as parametrized for the GFN methods.
- **Lange2012ALPBKernel**: Lange & Herbert, J. Chem. Theory Comput. 8, 1999 (2012),
  doi:10.1021/ct300111m. The P16 interaction kernel axtb uses for ALPB.
- **Sigalov2006ALPB**: Sigalov, Fenley & Onufriev, J. Chem. Phys. 124, 124902 (2006),
  doi:10.1063/1.2177251. The analytical linearized Poisson–Boltzmann model ALPB builds on.
- **Still1990GBSA**: Still et al., J. Am. Chem. Soc. 112, 6127 (1990),
  doi:10.1021/ja00172a038. The generalized Born surface area model GBSA builds on.

## Charges and bond orders

Calculations report Mulliken charges and Wiberg–Mayer bond orders for GFN2-xTB and g-xTB,
and EEQ charges for GFN-FF (Spicher2020GFNFF; `crates/xtb-worker/src/engine.rs`). The file
readers import Mulliken charges. Löwdin, Hirshfeld and CM5 are labels the data model
(`ChargeScheme` and `BondOrder` in `crates/qcdata/src/result.rs`) can carry; ReShiki neither
computes nor currently imports them.

- **Mulliken1955Population**: Mulliken, J. Chem. Phys. 23, 1833 (1955),
  doi:10.1063/1.1740588. Mulliken populations.
- **Lowdin1950Orthogonality**: Löwdin, J. Chem. Phys. 18, 365 (1950), doi:10.1063/1.1747632.
  Löwdin populations (data-model label).
- **Hirshfeld1977Charges**: Hirshfeld, Theor. Chim. Acta 44, 129 (1977),
  doi:10.1007/BF00549096. Hirshfeld charges (data-model label).
- **Marenich2012CM5**: Marenich et al., J. Chem. Theory Comput. 8, 527 (2012),
  doi:10.1021/ct200866d. Charge Model 5 (data-model label).
- **Wiberg1968BondOrder**: Wiberg, Tetrahedron 24, 1083 (1968),
  doi:10.1016/0040-4020(68)88057-3, and **Mayer1983BondOrder**: Mayer, Chem. Phys. Lett. 97,
  270 (1983), doi:10.1016/0009-2614(83)80005-0. Wiberg–Mayer bond orders.

## Optimization

ReShiki's own optimizer, `crates/optimize` (MIT OR Apache-2.0, written from the papers).
Paths below are relative to `crates/optimize/src`.

- **Wang2016TRIC**: Wang & Song, J. Chem. Phys. 144, 214108 (2016), doi:10.1063/1.4952956.
  Translation–rotation–internal coordinates, the default coordinate system
  (`optimizer/internal.rs`, `optimizer/internal/fragment.rs`).
- **Baker1996DLC**: Baker, Kessi & Delley, J. Chem. Phys. 105, 192 (1996),
  doi:10.1063/1.471864. Delocalized internal coordinates (`optimizer/internal.rs`,
  `optimizer/internal/metric.rs`).
- **Bakken2002Coordinates**: Bakken & Helgaker, J. Chem. Phys. 117, 9160 (2002),
  doi:10.1063/1.1515483. Primitive internal coordinates, their Wilson B rows and the Lindh
  parameters (`optimizer/internal/primitive.rs`, `optimizer/internal/topology.rs`,
  `model_hessian.rs`).
- **Peng1996Redundant**: Peng et al., J. Comput. Chem. 17, 49 (1996),
  doi:10.1002/(SICI)1096-987X(19960115)17:1<49::AID-JCC5>3.0.CO;2-0. Redundant internal
  coordinates and the back-transformation (`optimizer.rs`, `optimizer/internal.rs`,
  `optimizer/internal/back_transform.rs`).
- **Pulay1992BackTransform**: Pulay & Fogarasi, J. Chem. Phys. 96, 2856 (1992),
  doi:10.1063/1.462844. Iterative back-transformation (`optimizer/internal/back_transform.rs`).
- **Wilson1955Vibrations**: Wilson, Decius & Cross, _Molecular Vibrations_, McGraw-Hill
  (1955); Dover reprint (1980), ISBN 978-0-486-63941-3. Out-of-plane coordinate derivatives
  (`optimizer/internal/primitive.rs`); also the TS normal modes.
- **Lindh1995Hessian**: Lindh et al., Chem. Phys. Lett. 241, 423 (1995),
  doi:10.1016/0009-2614(95)00646-L. The default starting model Hessian of both RFO methods
  (`model_hessian.rs`, `optimizer/internal/topology.rs`).
- **Packwood2016Contacts**: Packwood et al., J. Chem. Phys. 144, 164109 (2016),
  doi:10.1063/1.4947024. Distance weight of the optional nonbonded contact curvature
  (`model_hessian/contacts.rs`).
- **Pyykko2009Radii**: Pyykkö & Atsumi, Chem. Eur. J. 15, 186 (2009),
  doi:10.1002/chem.200800987, and **Pyykko2009Double**: Pyykkö & Atsumi, Chem. Eur. J. 15,
  12770 (2009), doi:10.1002/chem.200901472. Covalent radii for the bond graph
  (`optimizer/internal/topology.rs`, `model_hessian/contacts.rs`; also the TS endpoints).
- **Mantina2009Radii**: Mantina et al., J. Phys. Chem. A 113, 5806 (2009),
  doi:10.1021/jp8111556. Van der Waals radii (`optimizer/internal/topology.rs`,
  `model_hessian/contacts.rs`).
- **Banerjee1985PRFO**: Banerjee et al., J. Phys. Chem. 89, 52 (1985),
  doi:10.1021/j100247a015. Rational-function steps (`optimizer/rfo.rs`,
  `optimizer/rfo/positive.rs`, `optimizer/anc.rs`, `optimizer/internal.rs`).
- **Grimme2017ANCopt**: Grimme, Bannwarth & Shushkov, J. Chem. Theory Comput. 13, 1989
  (2017), doi:10.1021/acs.jctc.7b00118. Approximate normal coordinates as in xtb's ANCopt
  (`optimizer/anc.rs`).
- **Davidson1975Eigen**: Davidson, J. Comput. Phys. 17, 87 (1975),
  doi:10.1016/0021-9991(75)90065-0. Lowest eigenvector of the RFO matrix
  (`optimizer/anc/davidson.rs`).
- **Farkas1999Update**: Farkas & Schlegel, J. Chem. Phys. 111, 10806 (1999),
  doi:10.1063/1.480484. SR1/BFGS blended Hessian update, the optional `Update::Bofill`
  (`optimizer.rs`, `optimizer/rfo.rs`).
- **Powell1978Damping**: Powell, in _Numerical Analysis_ (ed. Watson), Lecture Notes in
  Mathematics, 144 (1978), doi:10.1007/BFb0067703. Damped BFGS update (`optimizer.rs`,
  `optimizer/rfo.rs`, via Nocedal & Wright, Procedure 18.2).
- **Csaszar1984GDIIS**: Császár & Pulay, J. Mol. Struct. 114, 31 (1984),
  doi:10.1016/S0022-2860(84)87198-7, **Farkas2002GDIIS**: Farkas & Schlegel, Phys. Chem.
  Chem. Phys. 4, 11 (2002), doi:10.1039/B108658H, and **Li2006GEDIIS**: Li & Frisch, J. Chem.
  Theory Comput. 2, 835 (2006), doi:10.1021/ct050275a. Optional GDIIS and GEDIIS steps
  (`optimizer/rfo/diis.rs`, `optimizer/anc/diis.rs`).
- **SanzGarcia2016Thesis**: J. Sanz García, PhD thesis, Université de Toulouse (2016),
  doi:10.70675/9b2808d6z1282z4702zbe8cz2edf56ef312c, HAL tel-01402497. The GEDIIS
  switching thresholds (`Acceleration::Gediis` in `optimizer.rs`).
- **Liu1989LBFGS**: Liu & Nocedal, Math. Program. 45, 503 (1989), doi:10.1007/BF01589116,
  and **Armijo1966LineSearch**: Armijo, Pacific J. Math. 16, 1 (1966),
  doi:10.2140/pjm.1966.16.1. Cartesian L-BFGS direction (`optimizer/lbfgs.rs`) and Armijo
  backtracking (`search_lbfgs` in `optimizer.rs`).
- **Nocedal2006Numerical**: Nocedal & Wright, _Numerical Optimization_, 2nd ed., Springer
  (2006), ISBN 978-0-387-30303-1, doi:10.1007/978-0-387-40065-5. L-BFGS two-loop recursion
  (Algorithm 7.4), trust radius (Algorithm 4.1), BFGS curvature test and damping
  (`optimizer/lbfgs.rs`, `optimizer/trust.rs`, `optimizer/rfo.rs`).
- **Golub2013Matrix**: Golub & Van Loan, _Matrix Computations_, 4th ed., Johns Hopkins
  University Press (2013), ISBN 978-1-4214-0794-4, doi:10.56021/9781421407944. Symmetric eigensolver and Krylov shift
  (`linalg.rs`, `optimizer/rfo/positive.rs`); also the Jacobi principal axes of the symmetry
  number (`crates/qcdata/src/vibrational/symmetry.rs`).
- **Horn1987Quaternion**: Horn, J. Opt. Soc. Am. A 4, 629 (1987),
  doi:10.1364/JOSAA.4.000629. Quaternion rotations of TRIC fragments and conformer
  superposition (`optimizer/internal/fragment.rs`, `conformers/alignment.rs`; also the TS
  atom mapping).

## Conformer search

The geometry worker (`native/geometry`, COSMolKit) embeds the starts. With MMFF94s, MMFF94
or UFF it also minimizes them; with GFN-FF or GFN2-xTB selected, xTB minimizes them instead.
`crates/optimize/src/conformers` and `src/app/xtb/conformers/pipeline.rs` cluster the
minimized starts and re-rank the representatives. Graph automorphisms for symmetry-aware RMSD
(`conformers/topology.rs`) and Boltzmann populations (`conformers/ranking.rs`) follow no
specific paper.

- **Riniker2015ETKDG**: Riniker & Landrum, J. Chem. Inf. Model. 55, 2562 (2015),
  doi:10.1021/acs.jcim.5b00654, and **Wang2020ETKDGv3**: Wang et al., J. Chem. Inf. Model.
  60, 2044 (2020), doi:10.1021/acs.jcim.0c00025. ETKDGv3 embedding of the starts.
- **Halgren1996MMFF**: Halgren, J. Comput. Chem. 17, 490 (1996),
  doi:10.1002/(SICI)1096-987X(199604)17:5/6<490::AID-JCC1>3.0.CO;2-P. MMFF94.
- **Halgren1999MMFF94s**: Halgren, J. Comput. Chem. 20, 720 (1999),
  doi:10.1002/(SICI)1096-987X(199905)20:7<720::AID-JCC7>3.0.CO;2-X. MMFF94s, the default
  pre-minimizer.
- **Rappe1992UFF**: Rappé et al., J. Am. Chem. Soc. 114, 10024 (1992),
  doi:10.1021/ja00051a040. UFF, when chosen, and in place of MMFF for molecules with a
  metal.
- **Tosco2014MMFF**: Tosco, Stiefl & Landrum, J. Cheminform. 6, 37 (2014),
  doi:10.1186/s13321-014-0037-3. RDKit's MMFF implementation, the source of COSMolKit's
  parameter tables and ReShiki's comparison reference (`docs/geometry-backend.md`).
- **Butina1999Clustering**: Butina, J. Chem. Inf. Comput. Sci. 39, 747 (1999),
  doi:10.1021/ci9803381. Clustering of minimized starts by heavy-atom RMSD
  (`conformers/clustering.rs`).
- **Horn1987Quaternion** (see Optimization): RMSD superposition.

## Frequencies and thermochemistry

axtb computes the Hessian, frequencies and IR intensities; ReShiki recomputes the
thermochemistry for other temperatures, scales and cutoffs (`thermochemistry` in
`crates/qcdata/src/vibrational.rs`).

- **Grimme2012QRRHO**: Grimme, Chem. Eur. J. 18, 9955 (2012), doi:10.1002/chem.201200497.
  Quasi-RRHO entropy of low-frequency modes.
- **FernandezRamos2007Symmetry**: Fernández-Ramos et al., Theor. Chem. Acc. 118, 813 (2007),
  doi:10.1007/s00214-007-0328-0. Rotational symmetry numbers in the rotational partition
  function. With default masses axtb takes the number from its point group; when a run
  carries explicit isotope masses, ReShiki counts the proper rotations of the
  isotope-labelled geometry itself (`rotational_symmetry_number` in
  `crates/qcdata/src/vibrational.rs` and `vibrational/symmetry.rs`, using Jacobi rotations
  (Golub2013Matrix) and augmenting-path atom matching (Kuhn1955Hungarian); called from
  `src/app/xtb/run.rs`).
- **Eckart1935Conditions**: Eckart, Phys. Rev. 47, 552 (1935), doi:10.1103/PhysRev.47.552,
  and **Miller1980Eckart**: Miller, Handy & Adams, J. Chem. Phys. 72, 99 (1980),
  doi:10.1063/1.438959. Projection of rigid translations and rotations from the Hessian
  (TS normal modes, `crates/optimize/src/ts/modes.rs`).

## IR spectrum

The spectrum (`ir_spectrum` and `BandShape` in `crates/qcdata/src/vibrational.rs`, the figure
in `src/app/xtb/vibrations/figure.rs`) broadens axtb's stick intensities with area-normalized
Lorentzian or Gaussian bands. These are standard line shapes and need no citation.

## Orbitals

Orbital coefficients and the basis come from axtb (`crates/xtb-worker/src/ao.rs`,
`crates/qcdata/src/basis.rs`). Surfaces drawn into the document live in
`crates/model/src/models3d/surface/`.

- **Lorensen1987MarchingCubes**: Lorensen & Cline, ACM SIGGRAPH Comput. Graph. 21, 163
  (1987), doi:10.1145/37402.37422. Isosurface extraction (`surface/mesh.rs`). ReShiki uses
  the marching-tetrahedra variant, cutting each cube into six tetrahedra, usually credited
  to Doi & Koide (1991; see [Verification notes](#verification-notes)).
- **Taubin1995Smoothing**: Taubin, Proc. SIGGRAPH '95, 351 (1995),
  doi:10.1145/218380.218473. Smoothing without shrinkage (`surface/mesh.rs`).
- **Garland1997Simplification**: Garland & Heckbert, Proc. SIGGRAPH '97, 209 (1997),
  doi:10.1145/258734.258849. Quadric error simplification (`surface/mesh.rs`).
- **Cyrus1978Clipping**: Cyrus & Beck, Comput. Graph. 3, 23 (1978),
  doi:10.1016/0097-8493(78)90021-3; **Sutherland1974Clipping**: Sutherland & Hodgman,
  Commun. ACM 17, 32 (1974), doi:10.1145/360767.360802; and **Douglas1973Simplification**:
  Douglas & Peucker, Cartographica 10, 112 (1973), doi:10.3138/FM57-6770-U75U-7727. Segment
  and polygon clipping and outline thinning for the visible parts of a surface
  (`surface/visible.rs`).
- **Schneider1990CurveFit**: Schneider, in _Graphics Gems_, Academic Press, 612 (1990),
  doi:10.1016/B978-0-08-050753-8.50132-7. Cubic outlines of the projected surface
  (`surface/fit.rs`).

## Transition states

The transition-state engine (`crates/optimize/src/ts/`, GFN2-xTB only; on the
`xtb/w2-ts-engine` branch until it merges). Keys match its code comments.

- **Smidstrup2014IDPP**: Smidstrup, Pedersen, Stokbro & Jónsson, J. Chem. Phys. 140, 214106
  (2014), doi:10.1063/1.4878664. Image-dependent pair potential path guess (`ts/idpp.rs`).
- **Henkelman2000CINEB**: Henkelman, Uberuaga & Jónsson, J. Chem. Phys. 113, 9901 (2000),
  doi:10.1063/1.1329672. Climbing-image NEB (`ts/neb.rs`).
- **Henkelman2000Tangent**: Henkelman & Jónsson, J. Chem. Phys. 113, 9978 (2000),
  doi:10.1063/1.1323224. Improved NEB tangent (`ts/neb.rs`).
- **Bitzek2006FIRE**: Bitzek et al., Phys. Rev. Lett. 97, 170201 (2006),
  doi:10.1103/PhysRevLett.97.170201. FIRE relaxation of the band (`ts/neb.rs`, `ts/idpp.rs`).
- **Banerjee1985PRFO** (see Optimization) and **Baker1986PRFO**: Baker, J. Comput. Chem. 7,
  385 (1986), doi:10.1002/jcc.540070402. Partitioned RFO saddle refinement (`ts/saddle.rs`).
- **Bofill1994Update**: Bofill, J. Comput. Chem. 15, 1 (1994), doi:10.1002/jcc.540150102.
  SR1/PSB Hessian update between rebuilds (`ts/saddle.rs`).
- **Fletcher1987Practical**: Fletcher, _Practical Methods of Optimization_, 2nd ed., Wiley
  (1987), ISBN 978-0-471-91547-8, doi:10.1002/9781118723203 (online edition, 2000). Trust
  radius from the model ratio (`ts/saddle.rs`).
- **PageMcIver1988IRC**: Page & McIver, J. Chem. Phys. 88, 922 (1988), doi:10.1063/1.454172.
  Intrinsic reaction coordinate steps (`ts/irc.rs`).
- **Eckart1935Conditions**, **Miller1980Eckart** and **Wilson1955Vibrations**: rigid-motion
  projection and normal modes (`ts/modes.rs`).
- **Horn1987Quaternion**, **Kuhn1955Hungarian**: Kuhn, Naval Res. Logist. Q. 2, 83 (1955),
  doi:10.1002/nav.3800020109, and **Munkres1957Assignment**: Munkres, J. Soc. Ind. Appl.
  Math. 5, 32 (1957), doi:10.1137/0105003. Rigid fits and atom mapping (`ts/mapping.rs`).
- **Pyykko2009Radii** and **Wang2016TRIC** (see Optimization): bond graph and restrained
  endpoint relaxation (`ts/endpoint.rs`).
- **Muller1979Surface**: Müller & Brown, Theor. Chim. Acta 53, 75 (1979),
  doi:10.1007/BF00547608. The Müller–Brown test surface (`ts/tests.rs`).

## Units and constants

- **Mohr2025CODATA**: Mohr et al., Rev. Mod. Phys. 97, 025002 (2025),
  doi:10.1103/RevModPhys.97.025002. CODATA 2022 constants for unit conversions and
  thermochemistry (`crates/qcdata/src/lib.rs`, `crates/qcdata/src/vibrational.rs`,
  `crates/optimize/src/lib.rs`, `crates/xtb-worker/src/protocol.rs`).

## File formats

Readers in `crates/io/src/qcread/`.

- **MoldenFormat**: "Molden Format", <https://www.theochem.ru.nl/molden/molden_format.html>,
  and **Schaftenaar2000Molden**: Schaftenaar & Noordik, J. Comput.-Aided Mol. Des. 14, 123
  (2000), doi:10.1023/A:1008193805436. Molden files (`qcread/molden.rs`).
- **GaussianFchk**: Gaussian 09 User's Reference, <https://gaussian.com/interfacing/>.
  Formatted checkpoint files (`qcread/fchk.rs`).
- **Neese2025ORCA6**: Neese, WIREs Comput. Mol. Sci. 15, e70019 (2025),
  doi:10.1002/wcms.70019. ORCA 6 outputs (`qcread/orca.rs`).
- **OBoyle2008cclib**: O'Boyle, Tenderholt & Langner, J. Comput. Chem. 29, 839 (2008),
  doi:10.1002/jcc.20823. cclib, whose BSD-3-Clause test files are ReShiki's reader fixtures.
- **Sun2020PySCF**: Sun et al., J. Chem. Phys. 153, 024109 (2020), doi:10.1063/5.0006074.
  PySCF, whose Molden exports confirmed the orbital conventions.

## Software

- **ReShiki**: <https://github.com/Ameyanagi/ReShiki>, MIT OR Apache-2.0. Cite the release
  you used.
- **Axtb**: axtb, <https://github.com/atomsforhumanity/axtb>, LGPL-3.0-or-later. ReShiki
  links the fork <https://github.com/Ameyanagi/axtb> at the revision in `AXTB_REV`
  (`crates/xtb-worker/src/lib.rs`).
- **XtbProgram**: xtb, <https://github.com/grimme-lab/xtb>, and **Tblite**: tblite,
  <https://github.com/tblite/tblite>. Reference programs that axtb reproduces.
- **RDKit**: <https://www.rdkit.org>, doi:10.5281/zenodo.591637 (Zenodo concept DOI).
- **COSMolKit**: <https://github.com/Ameyanagi/COSMolKit> (0.5.0, fork of
  <https://github.com/cosmol-studio/COSMolKit>). ETKDGv3, MMFF94/MMFF94s and UFF in the
  geometry worker.

## Verification notes

- Powell1978Damping: the Lecture Notes in Mathematics volume number (630 in older notes and
  according to a reviewer) is not in the Crossref or OpenAlex records, and Springer's page
  refused our automated fetch, so it is left out.
- Golub2013Matrix: the code does not name an edition; the 4th edition is assumed. Its
  Crossref record (doi:10.56021/9781421407944) gives the release year 2012 and lists only
  the first author; the 2013 copyright year and both authors follow Open Library.
- SanzGarcia2016Thesis: the Crossref record (registered by Abes) has no year; the year,
  school and title come from HAL, and printed page 54 of the thesis gives the 10⁻³ a.u. RMS
  force and 2.5·10⁻³ a.u. RMS step switches quoted in `optimizer.rs`.
- Marching tetrahedra: A. Doi & A. Koide, "An Efficient Method of Triangulating Equi-Valued
  Surfaces by Using Tetrahedral Cells", IEICE Trans. Inf. Syst. E74-D, 214–224 (1991), as a
  reviewer reported it. It has no Crossref record and the IEICE page refused our automated
  fetch, so it is not in xtb.bib; the cube split is cited through Lorensen1987MarchingCubes.
- Software entries and the Molden and Gaussian format pages have no DOI (except RDKit's
  DataCite DOI); their URLs were checked to respond.
- Froitzheim2025gxTB is a preprint; Crossref lists no journal version yet.
