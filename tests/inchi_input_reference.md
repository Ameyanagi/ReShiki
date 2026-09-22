The fixture records the input arrays passed by RDKit's `MolToInchi` to
`GetINCHI`. The oracle compiles the exact `fixOptionSymbol`, `rCleanUp` and
`MolToInchi` bodies from RDKit commit
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`. Only the generator and output-freeing
calls are replaced with observation functions; expected arrays are not
calculated in Python. Molecules cross the test boundary as native pickles,
including double-precision conformer coordinates and their original caches.

`build_inchi_input_oracle.py` verifies the adapter and official InChI 1.07.3
header hashes, retains both source notices, and records source hashes and
compiler arguments in `artifacts/inchi-input-build.json`. The checked fixture
was generated with Apple clang, C++20 and `-O2`. The BSD notice for the adapter
is in `licenses/rdkit/INCHI-ADAPTER`; the InChI MIT notice is in
`licenses/inchi/LICENSE`. This test-only C++ is never linked into the app.

Build with `--rdkit-source PATH --inchi-source PATH`, then run
`inchi_input_reference.py --oracle artifacts/inchi-input-oracle --write-fixture`.
`--verify-fixture` compares a fresh native run with the checked fixture. Normal
Rust tests replay the fixture through the locked Python reference environment.

The 11,886 cases cover templates and the installed NCI corpus, aromatic and
nonaromatic graph states, isotope/charge/H/radical fields, source bond ordering,
perchlorate cleanup, all bond-direction and stereo codes, conformer absence,
explicit zero coordinates, atom permutations and invalid annotations.
There are 11,772 matching arrays, 64 matching native failures, and 50 explicit
rejections of undefined native inputs: tetrahedral annotations with total
degree three or four but fewer than three graph neighbors leave native
neighbor slots uninitialized. These cases are not executed by the C++ oracle.

The Rust library prepares owned input only. Calling a generator, its chemical
acceptance rules and its 1,023-atom limit remain separate work. The input ABI
stores atom and stereo counts in signed 16-bit fields; stored isotope masses
and H counts retain the native integer narrowing behavior. No application
code uses C++ or FFI.
