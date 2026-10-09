# Windows native EMF acceptance — 2026-10-10

This adds actual Windows desktop evidence to the earlier Mac and transport-harness review. Original controlled test data, screenshots and review records by @Ameyanagi, **MIT OR Apache-2.0**. Prior public fixtures, Mac/source/build/test receipts and images remain byte-original.

Through Windows App/RDP, the operator chose the same `source.emf` using **Import → Choose file**. The baseline rejected it with `Can't import .emf`; the candidate imported an EMF picture at **100 × 60 mm**, with a **4724 × 2834** preview. Native Save and EMF Export created the files below. The first candidate process exited; a fresh process reopened that actual native save with the same picture dimensions and disabled Undo/Redo.

| Artifact                                                                     | Meaning                                                                                                                                                                                     |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [source.emf](source.emf)                                                     | Exact 12,860-byte controlled input; identical to the earlier `controlled-spectrum.emf`, SHA256 `5c59f0c6…5be6a`.                                                                            |
| [desktop-import-20261010.rsk](desktop-import-20261010.rsk)                   | Actual GUI save, 492,401 bytes, SHA256 `7a689884…0158f`; version 20, one picture, exact original EMF and canonical PNG. Only placement origin differs from the earlier harness native file. |
| [desktop-vector-reexport-20261010.emf](desktop-vector-reexport-20261010.emf) | Actual GUI export, 14,704 bytes, SHA256 `a1c18d84…70c1a`; 422 records, **102.82 × 62.82 mm** padded figure.                                                                                 |
| [native-verification.json](native-verification.json)                         | Byte-original root desktop receipt, SHA256 `ea20953eaa1edf697a6680d5307859542e8317f43d8a95c9ef8213d8db0318e3`.                                                                              |
| [evidence-manifest.json](evidence-manifest.json)                             | Full hashes, lengths and original receipt locations for all 24 byte-exact copies, totaling 2,799,658 bytes. Scratch paths document provenance; verification does not require them.          |

The exported nested metafile retains all **23 DrawLines and 16 DrawString** source records exactly, plus the same raster image object and placement. Its outer stream contains **23 POLYLINE16, 16 EXTTEXTOUTW and one STRETCHDIBITS** fallback records. This establishes that the export retains vectors/text as well as the raster inset, independently of the screenshot. The export differs from the older harness output in seven records, including transformations and fallback font/text fields; not every difference is attributed to placement alone. The picture's 100 × 60 mm size and padded export's 102.82 × 62.82 mm frame are different quantities.

## Raw desktop images

All five JPEGs are original **2556 × 1712** captures with no cropping, resizing or annotation. The baseline was maximized to reveal the footer; fresh reopen has different framing. Screenshot scale is not used as a physical-size oracle.

- [Baseline rejection](../../../../images/emf-import/emf-import-baseline-unsupported.jpg): preserved baseline PID **13752**, red import error, empty canvas.
- [Candidate original dimensions](../../../../images/emf-import/emf-import-candidate-original-dimensions.jpg): PID **5852**, source type and physical/preview dimensions.
- [Candidate native save](../../../../images/emf-import/emf-import-candidate-native-saved.jpg): actual saved filename in the tab; retained file bytes independently checked.
- [Candidate export panel](../../../../images/emf-import/emf-import-candidate-vector-export-completed.jpg): actual export workflow; saved output independently checked.
- [Fresh-process reopen](../../../../images/emf-import/emf-import-candidate-fresh-process-reopened.jpg): new PID **4748**, original picture size and disabled Undo/Redo; old PID 5852 absent and file hash unchanged.

The earlier snap-layout-overlay image and RDP-host-only accessibility dumps remain scratch evidence, excluded from these selected public copies. The root receipt lists them to preserve the complete capture history; its `files` list is not the public package manifest.

## Executable and source attribution

The full GUI candidate executable SHA256 is `124336a60753ad7cd3bba0c9bf4f453298d6fac7d857dbe1562903e2e0fc88b2`. [app-link-inputs.json](app-link-inputs.json) and [emf-import-source-v3.json](emf-import-source-v3.json) preserve the original source-v3 build record: a precommit snapshot on base `51fa0991`, with the exact Cargo-captured compiler inputs replayed and only owned output/PDB locations changed. The 2,559-file source receipt was checked before the historical link. This package's optional Git check independently matches **1,120 inputs** (`*.rs`, all `Cargo.toml`, root `Cargo.lock`, `assets/`, `presets/`) to production commit **`783be63a2752103cd2c436a79e02a03d442e6085`**. It does not claim that the original build was invoked from that committed checkout.

Review head **`692d4c534cdbe2cb74332a22d87f226039360422`** differs only in tests and documentation/evidence. Under `src/`, `crates/`, `native/` and root build-input paths, the three differences are `crates/agent/tests/headless.rs` and two MCP test transcripts. The full diff additionally contains documentation/evidence and four top-level MCP transcript fixtures; it is not a three-file full-tree diff. Packaging makes no production changes.

The baseline executable SHA256 is `135779960a15c17bac51e00c614600422531a30b90480d6cb1ba8de5b795e51d`. Its historical source attribution is **`51fa0991da2507bb00b27c1b420e807468de6423`**, supported by the retained ledger, build command and successful log, not a reconstructed complete original compiler-input manifest. Current source staging was subsequently modified. [baseline-source-and-space-verification.json](baseline-source-and-space-verification.json) preserves this limit and the independently checked binary/process identity. Its disk-space figures are historical observations, not current build capacity or authorization.

The old failed app-link launch and subsequent relocated-link wrapper receipt remain unmodified. The latter's null exit code is not presented as a successful original Cargo run; the exact completed executable's subsequent worker receipt and desktop execution establish that it runs. Earlier `readiness.json` and worker receipts describe checks made before these desktop interactions. Prior terminal [Checks run 37935128139](https://github.com/Ameyanagi/ReShiki/actions/runs/37935128139) passed all native macOS, Windows and Linux jobs and all six geometry targets at `692d4c534cdbe2cb74332a22d87f226039360422`. Sway runtime passed separately; the GNOME job was cancelled before application build/runtime and is not an application result. Historical receipts retain their original pending-CI statements. No new compilation or CI run was performed to package this evidence.

For this initial package, the formatter configuration excludes the 11 exact raw JSON receipt paths that Oxfmt would rewrite (escape, layout or original encoding differences). The later Office supplement adds three individually listed raw receipt paths, for 14 total new exceptions. Their exact bytes and hashes are preserved; authored documentation, the manifest and verifier use normal formatting. No existing formatter exclusion was changed. Four exact `.gitattributes` entries disable text conversion only for the original CRLF build receipts; the global line-ending rule is unchanged, so their committed bytes also retain the recorded hashes.

## Portable verification

From the repository root, with Python 3 and no external packages:

```sh
python3 docs/changes/fixtures/emf-import/windows-native-20261010/verify.py
python3 docs/changes/fixtures/emf-import/windows-native-20261010/verify.py --source-root .
```

The first command checks all copied hashes, JPEG dimensions, same controlled input, native source/preview retention, unchanged picture axes, native physical dimensions, EMF frame/count/boundaries, nested source vector/text/raster equality and classic fallback records. Six in-memory negative controls must reject an altered embedded original, doubled picture width, one EMF frame-unit width change, changed nested vector, missing classic vector fallback and changed raw screenshot. Semantic controls call the artifact readers directly rather than failing only a whole-file hash check. The second command additionally compares the pinned source manifest and review delta to local Git objects; it does not build or contact a service.

This bounded artifact reader is not a general EMF safety validator, native renderer or replacement for the recorded GUI observation. The saved-byte/fresh-reopen check does not establish power-loss durability. The later [Office supplement](office-native/README.md) verifies actual Word/PowerPoint insertion, Save and fresh-process reopen; its separate manifest/verifier preserves this earlier package. Physical high-DPI/mixed-DPI coverage remains unverified. No issue-closing claim is made.
