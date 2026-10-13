# MOL XYZ / hydrogen-folding stereochemistry audit

Read-only specialist review, 2026-10-09. Worktree: `/Users/ryuichi/dev/ReShiki-worktrees/mol-import-3d`, `fix/mol-import-3d`. No builds, tests, GUI, network, or production-source edits were performed by this reviewer. This requested report is the only written artifact. Implementation and runtime validation remain with `plan_chemistry` / root.

## Result

The architecture is sound for the reader's supported tetrahedral and alkene stereo: capture chemical configuration from source coordinates before display conversion; fold H with neighbor-parity updates; persist chemical winding / alkene controls independently of projected appearance; rotate retained XYZ without deriving a new configuration from its XY projection. It does **not** establish support for enhanced relative/racemic stereo groups, which the existing reader rejects.

Two concrete defects were identified. The first was independently caught and fixed by the owner during this review. The second, loss of explicit-unknown alkene stereo, was sent to the owner and root and remained outstanding at the reviewed snapshot. No broader preservation claim should be made until that case passes through the full import/native/export path.

## Findings

### 1. Every-bond authority prevented importing ordinary 3D molecules — fixed during review

Initial `crates/io/src/chemistry/molfile/import.rs:222–227` assigned `stereo_authoritative = true` to every bond. `crates/model/src/document.rs:582–584` permits this flag only on double bonds. `crates/model/src/chemistry/document/output.rs:149–156` validates after the import callback. Thus any restored-XYZ molecule with a single bond, including the hexane fixture, failed before publication.

The current source restricts authority to `bond.order == 2` while retaining projection flags on orders 1/2/4. This satisfies the existing model invariant. The owner reports 19 focused MOL tests passing after this correction; that is owner-provided runtime evidence, not an audit execution.

### 2. Explicit-unknown double stereo becomes unspecified — confirmed source-path defect

Reproduction path: read a 3D V2000/V3000 alkene carrying explicit unknown double-bond stereo (`metadata.stereo == 1`), then `for_display()` → drawing / labels → `restore_xyz()` → `document::prepare()` (also after native save/reopen or rotation).

- `crates/model/src/chemistry/document/output.rs:594–601` depicts code 1 as `display = "wavy"`.
- The same file's `labeled()` at 186–192 and `stereo_name()` at 203–210 do not persist code 1 as native `stereo = "any"`; they leave `None`.
- `import.rs:222–229` adds projection and authoritative-double flags.
- `crates/model/src/chemistry/document.rs:214–223` suppresses directions for projected bonds, so the wavy appearance cannot restore `Direction::Unknown`.
- The wavy-double fallback at `document.rs:335–337` is explicitly disabled for authoritative doubles. The resulting metadata is stereo 0, and the preservation mask prevents subsequent geometry inference.

This loses the distinction between explicitly unknown and unspecified. It is not an R/S or E/Z inversion, but it is a stereochemical information loss. Existing `crates/io/src/chemistry/molfile/tests.rs:123–143` checks unknown-stereo MOL output directly from a molecule and bypasses drawing conversion, so it cannot detect this regression.

Recommendation: persist code 1 as native `stereo = "any"` before enabling authority, with valid controls (empty controls are explicitly supported by the native model). A common `stereo_name(1)` mapping or a narrowly scoped import restoration can do this; preserve unknown-vs-unspecified rather than dropping authority. Add end-to-end import/reopen/rotate/export assertions against metadata code 1, not only isomeric SMILES, which does not distinguish this case.

## Why the coordinate conversion is not a chirality inversion

Display coordinates are `q = A p`, with `A = 28 diag(1, -1, 1)` (`output.rs:509–512`, `import.rs:218`). The negative determinant reflects the screen-Y coordinate convention. It must not be mistaken for a physical reflection of the molecule: chemical tags are already assigned from source coordinates by `molfile/read.rs:380–398`.

`crates/model/src/projection.rs:124–138` applies proper X/Y rotation matrices `R` to display XYZ. Decoding back to source axes gives `A^-1 R A`, whose determinant is +1. Whole-molecule rotations therefore preserve physical handedness, pair distances and signed volume, subject to the existing f32 precision. Positive depth is documented as facing the viewer at `projection.rs:98–100`. Tests should compare coordinates after inverse screen conversion; comparing a raw screen-space signed volume with a source-space one produces a misleading sign change.

The native document persists depth, stable-neighbor winding, bond stereo controls, projection and authority (`crates/model/src/document.rs:22–26, 41–43, 63–64, 80–86`). Native chemical preparation reconstructs atom winding from stable IDs (`chemistry/document.rs:196–207`) and deliberately uses Z=0 (`229–233`); captured semantics, not the camera, determine chemical configuration. Partial-selection coordinate edits are not a rigid rotation of the whole source conformer and should not be used as the distance/volume invariant test.

## Hydrogen policy and supported stereo scope

The new eligibility gate is conservative (`import.rs:57–75`): only neutral, isotope-unspecified, nonradical, unmapped, nonunknown H with exactly one ordinary single bond to an allowed main-group parent can fold. D/T, charged/radical H, isolated H, metal hydrides, coordination bonds, multiattached/bridging H, dummy/attachment-bearing H and H on an explicitly unknown bond are protected. Attachment members are protected by stable ID at `40–49`.

The shared remover protects required terminal stereo H (`crates/chemistry/src/hydrogens.rs:144–175`), adjusts tetrahedral parity using the removed neighbor's position (`216–234`), and remaps alkene controls. The adapter verifies atom/H/charge/radical inventory and full CIP descriptors on kept atoms/bonds (`import.rs:147–168`), then remaps positions, IDs and per-atom/bond annotations together (`170–180`). This is substantially stronger than deleting H atoms by element alone. No concrete parity failure was found in the inspected implementation.

CIP/inventory comparison is a useful fail-closed check, but is not a proof that every stereo distinction is preserved: unknown-vs-unspecified may have no CIP descriptor, and enhanced relative groups are outside the accepted input domain. `molfile/read.rs:504–511` rejects non-tetrahedral tags and nonempty enhanced stereo groups. V3000 AND/OR/racemic group support must not be claimed. The unchanged `write_absolute()` also deliberately marks drawing stereocenters absolute (`molfile.rs:56–60`); this audit does not alter that existing policy or expand V2000 chiral-flag interpretation.

MOL interchange remains deliberately 2D: `molfile.rs:63–67, 102–105` generates a detached stereochemically valid depiction. It should preserve supported chemical configuration, not the source XYZ conformer. Native `.rsk` is the retained-XYZ round trip.

## Focused validation still needed

1. Explicit unknown versus unspecified alkene fixtures in both MOL dialects through read → H folding → native drawing → save/reopen → X and Y rotations → prepare → existing 2D MOL writer → independent reader. Assert code 1 / unknown metadata separately from SMILES. Include a wavy single bond at a potential tetrahedral center; its original source meaning must not be promoted to a definite configuration. That single-bond path is a test gap, not a separately proven new regression here.
2. Mirror-pair R/S and E/Z fixtures, plus a molecule with two supported stereocenters. Fold H in different atom/bond orders; reopen native files before checking chemistry. Current new tests cover one enantiomer, E, reordered H, and X-axis rotations; native reopen is asserted directly only on the alkane fixture. Compare independent reference stereochemistry, not merely the same reader/writer agreeing.
3. One fixture containing both removable ordinary H and protected H/annotations. Existing protected-only fixtures return before the removal/remapping path, so they do not exercise coexistence. Verify protected stable IDs, isotope/charge/radical state, directed metal bonds and coordinates while ordinary H disappear. Include a supported alkene whose control remapping involves explicit H.
4. Independently decode native `(x/28, -y/28, z/28)` and compare kept source coordinates, heavy-atom pair distances, signed tetrahedral volume or signed torsion after rotations about both axes. Root's existing headless RDKit reference and planned real GUI hexane check are appropriate complements. Test metadata after clearing cached CIP labels so cached presentation cannot hide lost semantics.

## Snapshot and evidence boundaries

Initial `import.rs` SHA-256: `a7c07ea30655db9ca3549490be5de674b2d9fa00aaf3093a116ff36471a5e41e`.

Latest source snapshot read after the authority correction:

- `import.rs`: `9730625891cd80d725c15eb079c66796c0d31c43d22870dcede73994a53b7593`.
- `import/tests.rs`: `027d434fe5eaacddc65a5e498d08a9af80e5135a0929d8208cec52557a36f5a9`.
- `engine/native_import.rs`: `0fd8e7d70ad3b47701d3bc70497f3331c17c2f9ae649d84b2ef91ec423c66d1e`.

The worktree is concurrently edited. Line references describe these reviewed files and may shift. The parent reported a 20-fixture source baseline; the owner reported 19 focused compiled tests after the first fix. This review did not rerun or independently certify those tests, a candidate GUI session, or the independent reference executable. Source inspection establishes the two paths above; runtime acceptance belongs to the Sol owner/root.
