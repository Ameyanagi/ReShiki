# ReShiki 0.8.0 release validation

[ReShiki 0.8.0](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.8.0) was published on September 25, 2026. This record covers its review against [0.7.1](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.7.1), commit `b25f0a4dd0fcce6aa99fe59a7f07a2552faa7470`, and verification of the published packages.

## Review scope

The review covers changed production code throughout the application, native document opening, drawing geometry, shortcuts, labels, chemical interchange, figure exports, packaging, licenses and the website. It includes the previously merged feature stack and release corrections in [PR #32](https://github.com/Ameyanagi/ReShiki/pull/32), [PR #34](https://github.com/Ameyanagi/ReShiki/pull/34), and [PR #36](https://github.com/Ameyanagi/ReShiki/pull/36). This is a differential review against the previous release, supported by runtime and regression checks.

Issues found and corrected during review include large PNG allocation failures, chemistry validation blocking figure exports, styled metal contacts becoming invalid during return import, misleading clipboard status, stale reference-test assumptions, colored three-way ring junction geometry, premature narrowing during ellipse recognition, and internal-label angle placement and group return import. The junction uses independently exported vector coordinates as well as raster checks; it does not infer success from a similar-looking full-size picture.

The [illustrated change log](changes-0.8.md) links the feature galleries and matched bug-fix captures. Groups and templates are available in their current form and may be revised in future releases.

## Desktop checks

Computer Use checks on the optimized macOS app cover the editable shortcut gallery, both ring gestures, element dragging, scrolling and zooming, optional guides, ligand movement and later tilt, formula text, native document opening and two-way editable clipboard transfer. PNG, SVG and PDF were saved from the full shortcut gallery and a drawing with unresolved aromatic assignments. PNGs were decoded and their physical-resolution metadata checked; PDF dimensions and raster rendering were checked independently.

The full gallery exported at 6627 × 5285 pixels and 300 dpi; its vector exports retain the same physical dimensions. Small drawings still export at 1200 dpi. Figure fallback retains the visible drawing and shows a chemistry-review notice. It does not declare the molecule chemically valid.

Internal NH, CH₂, CCl₂, CF₂ and NMe examples were compared across four orientations and copied as editable structures in both directions. The returned gallery retains 80 atoms, 60 bonds, 12 compact groups, formula C56H144Cl8F8N8 and its molecular identity. Forty-eight bent/straight angle cases and finer transition cases establish the tested placement behavior. [Matched captures and details](changes/automatic-hydrogen.md).

Existing user drafts were preserved. Graphical inspection was performed on macOS; Windows and Linux acceptance relies on their native CI and package checks, rather than a claim of local GUI inspection.

## Automated checks and publication

The local full native suite passed 673 tests with 3 existing ignored tests. This includes the crossing-clearance, rotated internal-label, real group-definition round-trip, and subsequent group-editing regressions. Cargo check, Clippy with warnings denied and formatting pass. The website builds 73 pages and verifies 5721 local links/assets with no diagnostics.

A fresh independent macOS capture of all 13,425 saved aromatic requests produces byte-identical response records. A gated three-platform capture job refreshes stale worker-source provenance and rejects any changed request/response hash before new captures are accepted.

The final source commit is `9229ce32ad7d266ec27eecf345b410238d30b9ab`.
The standard [PR checks](https://github.com/Ameyanagi/ReShiki/actions/runs/36129637053)
and independent [full live-reference run](https://github.com/Ameyanagi/ReShiki/actions/runs/36129658552)
passed on macOS, Windows and Linux. The reference run checked `b493b1753f93282d35e8698f838e032d051be6a5`,
whose source tree is identical to the final merge commit.

The [signed package rehearsal](https://github.com/Ameyanagi/ReShiki/actions/runs/36130641589)
built all five platform/architecture combinations. All eight package checksums
and five portable archive manifests were independently verified against the final
commit. The downloaded Mac archive also passed code-signature, Gatekeeper,
notarization-ticket and standalone native-chemistry checks after extraction to a
path containing spaces, with Python, uv and the source checkout unavailable.
The signed app opened the internal-label gallery successfully.

The annotated tag `v0.8.0` points to that final source commit. Its
[tagged release workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/36132257238)
passed the platform builds, signing and all twelve reference shards before
publication at 12:23 UTC on September 25, 2026.

## Published packages and website

All eight public packages were downloaded from the stable release and verified
against its `SHA256SUMS`. The five portable archive manifests identify version
0.8.0 and the final commit above, with the expected platform, architecture and
bundled native InChI helper.

| Platform | Architecture          | Published formats                                |
| -------- | --------------------- | ------------------------------------------------ |
| macOS    | Apple Silicon (ARM64) | Signed, notarized DMG and ZIP                    |
| Windows  | x64 and ARM64         | Setup EXE and portable ZIP for each architecture |
| Linux    | x64 and ARM64         | Portable tar.gz for each architecture            |

The public Mac ZIP is byte-identical to the signed artifact independently
verified from the tagged workflow. That verification checked its signature,
Gatekeeper acceptance, stapled notarization ticket and native chemistry without
Python, uv or the checkout. The release is public, marked stable, and selected
as the repository's latest release.

The live website displays **v0.8.0**. Browser checks at 1440-pixel and 390-pixel
widths passed for the home page, release notes, export evidence, shortcut guide,
sharing guide, Assistant guide/setup, installation guide and this validation
record: 18 page/viewport combinations, with no missing images, horizontal page
overflow or browser errors. The release notes link the editable examples,
visual changes and remaining interchange limitations.
