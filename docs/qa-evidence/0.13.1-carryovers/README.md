# ReShiki 0.13.1 carryover UX evidence

Actual screenshots captured from the official macOS ARM64 0.13.1 app on 27 October 2026 (macOS 27.0.1). No drawing or screenshot was retouched. Source release: 25ace5153fb112c636b7505eb319e029a00e09d7.

- crystal-tabs.png: co2.cif and rutile.vasp imported in separate tabs; both are Untitled.
- condition-editor.png: editing the upper NaBH4 condition at 109% obscures arrow/product context.
- condition-applied.png: same drawing after applying the condition.
- condition-repro.rsk: unchanged version-27 native input; edit its upper condition to NaBH4, MeOH.
- Crystal fixtures are copied unchanged from the tagged release.

Findings CR-3 and CO-2 are low-severity UX observations, not data loss. The native condition observation edits an existing caption; fresh condition creation was not part of this retest. This evidence branch contains no product code changes.
