# Actual Windows Word and PowerPoint acceptance

This supplement records insertion, native Save and fresh-process reopen of the exact Windows GUI EMF export in **Word and PowerPoint 16.0.20527.20032**. Original controlled data, containers, captures and records by @Ameyanagi, **MIT OR Apache-2.0**. Earlier receipts remain unchanged and describe their own observation times.

The operator inserted `desktop-vector-reexport-20261010.emf` through each application's native picture-file dialog into an owned blank document/presentation, without manually resizing. Both show the complete title, eight blue peaks, axes and red/green inset. The owned files were saved locally, closed and reopened in new processes. File hashes remained unchanged after reopen.

| Saved file                                                                                | Exact stored picture extent    | Fresh-process binding                                                                                   |
| ----------------------------------------------------------------------------------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------- |
| [PowerPoint](desktop-powerpoint-emf-20261010.pptx), 37,482 bytes, SHA256 `6ccd40ac…26563` | 3,701,880 × 2,261,880 EMU      | PID 13116, session 2, started 23:05:54Z; observed 23:09:58Z after earlier absent-process observations.  |
| [Word](desktop-word-emf-20261010.docx), 17,657 bytes, SHA256 `5fd87ef6…c25fc`             | Same picture and inline extent | Initial PID 5024; fresh PID 11920, session 2, started 23:22:44Z and observed 23:23:39Z; old PID absent. |

Both containers have one picture and one referenced embedded EMF, byte-identical to the actual exported `a1c18d84…70c1a` file. There is no crop, rotation or reflection. **Stored picture size is 102.83 × 62.83 mm**, versus the embedded EMF frame's **102.82 × 62.82 mm**: **+0.01 mm per axis**. The original imported ReShiki picture remains **100 × 60 mm**; export padding is a separate difference. Do not claim exact Office/source-frame equality or infer it from the screenshot controls. PowerPoint shows rounded 4.05 × 2.47 inches; Word shows width 102.82 and height 62.83 mm, while both XML extents are identical. The cause of this small discrepancy is not established. Word additionally stores a separate bottom effect/layout extent of 5,080 EMU (~0.1411 mm), not a change to picture height.

## Raw save and reopen captures

All four selected images are original **2556 × 1712** RDP screenshots, with no resize, crop, annotation or synthetic replacement. Word uses 200% document zoom and PowerPoint 68% slide zoom; image display size is not the physical-size oracle.

- [PowerPoint saved, size controls visible](../../../../../images/emf-import/office-native/powerpoint-emf-saved-intrinsic-size.jpg).
- [PowerPoint fresh-process reopen](../../../../../images/emf-import/office-native/powerpoint-emf-fresh-process-reopened.jpg).
- [Word saved, size controls visible](../../../../../images/emf-import/office-native/word-emf-saved-intrinsic-size.jpg).
- [Word fresh-process reopen](../../../../../images/emf-import/office-native/word-emf-fresh-process-reopened.jpg).

The misnamed `powerpoint-emf-inserted-intrinsic-size.jpg` actually captured a Save dialog and stays excluded in scratch. Earlier snap-overlay and failed/supplemental captures are retained separately. Accessibility dumps expose the RDP host and do not prove Office picture contents. Process receipts correlate timing and executable identity; visual contents are evidenced by the raw screenshots, without COM or worker GUI automation.

## Verify the retained evidence

[The manifest](evidence-manifest.json) pins all 14 byte-exact copies (1,443,592 bytes). Seven original SSH/container/process receipts retain exact executable hashes, live process times, stable reads and the initial file-sharing failure. The separate [root Office acceptance receipt](root-office-native-acceptance.json), SHA256 `f3fc32a93d21b419dab9a0c88525de461c762d528abab05e5df7e5595f4250a6`, binds actual GUI Save/fresh-reopen observations, the four selected screenshots and independent exact-picture ZIP/XML checks. Its preserved parser-correction note distinguishes the picture transform from PowerPoint’s unrelated group transform. Word was read with a read-only stream allowing its existing writer handle; no file was changed to obtain the hash. The actual Office executables and private user documents are not published.

```sh
python3 docs/changes/fixtures/emf-import/windows-native-20261010/office-native/verify.py
```

This standard-library-only reader checks raw hashes/JPEG dimensions, ZIP integrity, the actual picture relationship, exact embedded EMF, picture/inline extents, no crop/rotation/flip, Word's separate effect extent, unchanged saved/reopened bytes and screenshot/process receipt correlation. Eight in-memory negative controls reject a one-EMU extent change, crop, rotation and altered embedded source in each format. It is a retained-artifact check, not a new Office execution or general compatibility test. The parent package verifier separately checks the EMF's retained vector/text/raster instructions and source/build correspondence.

This completes the recorded controlled-fixture Word/PowerPoint insertion, Save and fresh-reopen checks. It does not establish physical high-DPI/mixed-DPI coverage, interactive caption/rotation/reflection in Office, power-loss durability or universal EMF compatibility. The PR remains under review without an issue-closing claim. Only exact new raw receipt paths that Oxfmt would rewrite are excluded from formatting; authored files follow normal formatting.
