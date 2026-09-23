# ReShiki 0.7 release validation

Release preparation follows [PR #17](https://github.com/Ameyanagi/ReShiki/pull/17). The [illustrated change log](changes-pr17.md) records all 37 feature updates and the existing desktop screenshots. Screenshots captured during development can show v0.6.1 in the status bar; that is their actual build label.

PR #17 merged as `afca56f` after the final review commit `47c664c` passed all applicable macOS, Windows, Linux, Python, web and documentation checks in [the final PR run](https://github.com/Ameyanagi/ReShiki/actions/runs/35857585578). The merged tree matches that tested commit. The full live chemistry reference suite additionally gates tagged publication.

## Final review

The final review added regression coverage for tracked-centroid figure preparation, projection-only bond emphasis at the CDXML boundary, and unfinished atom-label and assistant input during update restart. It also requires a fresh installer acknowledgement on retry.

The targeted Rust run passed 355 tests with three existing manual tests ignored: 175 library, 162 application, 13 attachment, four dummy/figure-export and one projection-exchange test. Formatting, all-target/all-feature Clippy with warnings denied, and Cargo check passed. Earlier Computer Use checks cover ChemDraw attachment/group round trips and the optimized ReShiki editing workflows; see [the validation record](drawing-tools-validation.md).

## Publication gates

The documentation build passed with zero diagnostics and 4,441 verified local links/assets. Browser checks covered seven updated pages at desktop and mobile widths (14 combinations), with all images loaded, no page errors or horizontal overflow, and a working full-size screenshot link. Changelog screenshots are now served as website image assets rather than GitHub file-view pages.

The tagged release workflow must pass all five platform package checks, Windows installer checks, the full live chemistry reference suite on macOS/Linux/Windows, and macOS signing/notarization/Gatekeeper checks before it publishes downloads and `SHA256SUMS`. Documentation is built, checked for local links/assets and deployed separately to GitHub Pages.

Package and deployment results will be recorded after these gates complete. No unsupported interchange or chemistry feature is promoted to validated support by packaging; the [0.7 release notes](changes-0.7.md) retain those limits.
