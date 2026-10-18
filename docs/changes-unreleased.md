# Unreleased changes

- **Under review — One editable abbreviation catalog:** every chemical abbreviation now comes from one catalog file of labels, SMILES, formulas, categories and full names. Matching rules, formulas and layouts are derived from each SMILES with ReShiki's own depiction, so adding a group is one entry and the per-platform geometry captures are retired. The 62 existing groups keep identical layouts. Aliases such as TBDMS, MPM and Trt now create their groups. [Catalog format](abbreviations.md#the-group-catalog) · PR_A · @Ameyanagi.

- **Under review — Protecting-group abbreviations:** typing common carbohydrate and protecting-group labels such as OBn, OTBS, OPMB, NHAc, NHBoc, OTr, OTf or the trichloroacetimidate OC(NH)CCl3 on an endpoint now creates the real group instead of text, so formula and mass match the expanded structure. Thirty-one presets were added with captured geometry for every supported platform. [PR #348](https://github.com/Ameyanagi/ReShiki/pull/348) · @Ameyanagi.

- **Merged — Intel Mac panic recovery:** preserve the allocation alignment required by the native unwinder so recoverable background panics do not crash the application. Native Intel comparison reproduces the original crash and passes with the corrected alignment; global-allocator regression coverage now runs on all six supported native targets. [Diagnostic evidence](https://github.com/Ameyanagi/ReShiki/actions/runs/38822804581) · [PR #328](https://github.com/Ameyanagi/ReShiki/pull/328) · @Ameyanagi.

- **Merged — Nightly Windows startup:** use Direct3D 12 consistently during automatic adapter discovery and renderer initialization, retaining explicit backend preferences and the Tiny Skia fallback. The candidate opens the editor in the Windows sandbox that reproduced the original startup failure. Installer antivirus detections remain under vendor review; this is not an antivirus-clearance claim. [Investigation and limits](release-0.12.0-windows-security.md) · [PR #327](https://github.com/Ameyanagi/ReShiki/pull/327) · @Ameyanagi.

- **Under review — 0.12.0 release validation:** record the tagged source, the tag runs that did not publish, the publishing run, public-download checksums and macOS signing and package checks. [Validation record](release-0.12.0-validation.md) · [PR #324](https://github.com/Ameyanagi/ReShiki/pull/324) · @Ameyanagi.

See [ReShiki 0.12.0](changes-0.12.md) for the latest release notes.
