# ReShiki 0.8.0 release validation

This record tracks the release candidate against [0.7.1](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.7.1), commit `b25f0a4dd0fcce6aa99fe59a7f07a2552faa7470`. Publication and package verification are still pending. The intended stable version is 0.8.0.

## Review scope

The review covers changed production code throughout the application, native document opening, drawing geometry, shortcuts, labels, chemical interchange, figure exports, packaging, licenses and the website. It includes the previously merged feature stack and [PR #32](https://github.com/Ameyanagi/ReShiki/pull/32). This is a differential review against the previous release, supported by runtime and regression checks.

Issues found and corrected during review include large PNG allocation failures, chemistry validation blocking figure exports, styled metal contacts becoming invalid during return import, misleading clipboard status, stale reference-test assumptions, and colored three-way ring junction geometry. The junction uses independently exported vector coordinates as well as raster checks; it does not infer success from a similar-looking full-size picture.

The [illustrated change log](changes-0.8.md) links the feature galleries and matched bug-fix captures. Groups and templates are available in their current form and may be revised in future releases.

## Desktop checks

Computer Use checks on the optimized macOS app cover the editable shortcut gallery, both ring gestures, element dragging, scrolling and zooming, optional guides, ligand movement and later tilt, formula text, native document opening and two-way editable clipboard transfer. PNG, SVG and PDF were saved from the full shortcut gallery and a drawing with unresolved aromatic assignments. PNGs were decoded and their physical-resolution metadata checked; PDF dimensions and raster rendering were checked independently.

The full gallery exported at 6627 × 5285 pixels and 300 dpi; its vector exports retain the same physical dimensions. Small drawings still export at 1200 dpi. Figure fallback retains the visible drawing and shows a chemistry-review notice. It does not declare the molecule chemically valid.

Existing user drafts were preserved. Graphical inspection was performed on macOS; Windows and Linux acceptance relies on their native CI and package checks, rather than a claim of local GUI inspection.

## Automated checks and publication

The local full native suite passed 659 tests with 3 existing ignored tests. The final junction update adds a further crossing-clearance regression; all 15 join tests and 25 focused drawing, Haworth, crossing and figure-export integrations pass. Cargo check, Clippy with warnings denied and formatting pass. The website builds 73 pages and verifies 5721 local links/assets with no diagnostics.

A fresh independent macOS capture of all 13,425 saved aromatic requests produces byte-identical response records. A gated three-platform capture job refreshes stale worker-source provenance and rejects any changed request/response hash before new captures are accepted.

Final cross-platform checks, signed package rehearsal, tagged publication, public checksums and website deployment are recorded here when complete. No stable-release claim is made until those checks pass.
