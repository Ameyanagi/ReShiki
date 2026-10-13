# Rear visibility correction evidence

This package accompanies the [review guide](../../../docs/changes/rear-visibility-20261010.md)
for PR #289. It contains seven byte-exact native JPEGs in
[`docs/images/rear-visibility-20261010`](../../../docs/images/rear-visibility-20261010),
three actual native saves here, and 22 original JSON/log records. No raw AX,
file-picker images, scratch galleries or executable payloads are included.
Existing earlier evidence remains untouched.

Run from the repository root using Python's standard library:

```sh
python3 tests/fixtures/rear-visibility-20261010/verify.py
```

The verifier checks all 32 copied original files against pinned SHA-256 values,
all seven JPEG dimensions and the untouched original 1,071-input source
receipt. Current files must match 1,070 original hashes plus one exact old→new
Windows-only test hash pinned in `WINDOWS_TEST_DELTA`; no other source change is
allowed. All 13 correction files and the 529 distinct sources referenced by the
captured Mac shipping depfiles remain exact. Those depfiles contain 917 source
entries and 64 embedded-file entries (34 distinct files); the changed Windows
test is not a shipping dependency. The verifier also checks 15 own `fresh:false`
compiler records and the linking source/package/native receipt fields. It does not rerun compilation,
verify a missing local app signature, or recreate a desktop action.

The three native files are complete version-22 drawings: 60 neutral carbon
atoms, 90 bonds (60 single and 30 double), and no added hydrogen labels. The
25%→0% difference is only `rear_opacity`. The tilted save changes exactly
180 X/Y/depth values, agrees with the expected rigid X-then-Y rotation, and
preserves every other field. Semantic checks also exercise independent in-memory
corruptions of atom identity, isotope, charge, hydrogen label, bond order,
display, opacity and coordinates, plus reflection and uniform scaling. A
separate byte corruption control checks the hash gate. Three additional
controls reject the old Windows test, a shipping-source edit and an altered
original-test linkage. These are finite C60 reference checks, not a general chemistry or rendering validator.

`before-capture-provenance.json` binds the earlier implementation's signed
`4d35fe61…` app and 25% drawing to the matched before image.
`frozen-source-receipt.json`, `compiler-artifact-origin.json` and
`signed-app-provenance.json` bind the corrected source to signed `bc9b3d61…`.
`root-native-acceptance.json` records the later native 25%/0%/tilt, Undo/Redo and
fresh-process checks. `independent-native-semantic-audit.json` provides the
separate deep native comparison.

The original compiler/source receipts say native review was pending because
they were written before that review. The later ROOT receipt records the native
pass; it still correctly says publication/new-head CI is pending. Preserve those
original statements and bytes. `compiled-validation-summary.json` identifies
zero-test routes and unsuccessful depfile-parser attempts as excluded or
supplemental. The four small original test logs corroborate the 26 accepted
named passes without copying large build logs. Their terminal blank lines are
original bytes.

At published head `dd26cae383085af1f547b91578431a01f544c812`,
[Windows CI](https://github.com/Ameyanagi/ReShiki/actions/runs/38017370974/job/114110577818)
failed because the alpha test expected partial transparency on an exposed
three-atom chain. The test-only revision instead uses the existing complete C60
fixture for partial alpha/EMF rejection, and separately requires that exposed
chain paint stays opaque and produces an EMF with painted vector geometry and a
complete EOF record. It changes no production
renderer or captured Mac app dependency. Actual Windows execution of the
revised tests is pending; these portable checks do not establish that result.
Original source, compiler, test logs and native receipts remain unchanged.

The native tilted fresh-reopen image is at 203% automatic fit, versus 209% for
the other captures. Native checks are macOS/C60 examples, not universal visibility
certification. The guide discloses capture and export limits. All existing
project licenses and contributor attribution remain unchanged; the original
documentation/verifier contribution is offered under MIT OR Apache-2.0.

The visibility implementation retains these preflight limits: 256 vertices per
cage, 24 vertices per face, 768 triangles, 500,000 triangle-pair comparisons,
1,000,000 line queries, 100,000 ink edges, 100,000,000 estimated ink work and
200,000 crossing/broad-phase comparisons. See the [source](../../../crates/model/src/rear_opacity/visibility.rs).
These conservative estimates can fall back to wholly opaque output; they do
not certify the total number of operations after geometry splitting or the
correct visibility of every arbitrary molecule.
