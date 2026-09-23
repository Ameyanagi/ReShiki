# ReShiki 0.7 release validation

Release preparation follows [PR #17](https://github.com/Ameyanagi/ReShiki/pull/17). The [illustrated change log](changes-pr17.md) records all 37 feature updates and the existing desktop screenshots. Screenshots captured during development can show v0.6.1 in the status bar; that is their actual build label.

PR #17 merged as `afca56f` after the final review commit `47c664c` passed all applicable macOS, Windows, Linux, Python, web and documentation checks in [the final PR run](https://github.com/Ameyanagi/ReShiki/actions/runs/35857585578). The merged tree matches that tested commit. The full live chemistry reference suite additionally gates tagged publication.

## Final review

The final review added regression coverage for tracked-centroid figure preparation, projection-only bond emphasis at the CDXML boundary, and unfinished atom-label and assistant input during update restart. It also requires a fresh installer acknowledgement on retry.

The targeted Rust run passed 355 tests with three existing manual tests ignored: 175 library, 162 application, 13 attachment, four dummy/figure-export and one projection-exchange test. Formatting, all-target/all-feature Clippy with warnings denied, and Cargo check passed. Earlier Computer Use checks cover ChemDraw attachment/group round trips and the optimized ReShiki editing workflows; see [the validation record](drawing-tools-validation.md).

## Publication gates

The documentation build passed with zero diagnostics and verified local links/assets. Browser checks covered seven updated pages at desktop and mobile widths (14 combinations), with all images loaded, no page errors or horizontal overflow, and a working full-size screenshot link. Changelog screenshots are now served as website image assets rather than GitHub file-view pages.

The tagged release workflow must pass all five platform package checks, Windows installer checks, the full live chemistry reference suite on macOS/Linux/Windows, and macOS signing/notarization/Gatekeeper checks before it publishes downloads and `SHA256SUMS`. Documentation is built, checked for local links/assets and deployed separately to GitHub Pages.

[ReShiki v0.7.0](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.7.0) was published from `801178c6c103dd41db13f1d162b40b9e0f6acaad` after [the successful tagged workflow](https://github.com/Ameyanagi/ReShiki/actions/runs/35864186748) passed every gate: five platform package builds, Windows installers, all twelve live reference shards (four each on macOS, Linux and Windows), native Rust and Python checks on all three systems, web checks, and macOS signing/notarization/Gatekeeper verification. The independent [main-branch checks](https://github.com/Ameyanagi/ReShiki/actions/runs/35864178712) also passed. An earlier [signed packaging rehearsal](https://github.com/Ameyanagi/ReShiki/actions/runs/35859124439) passed before tagging.

All eight public downloads were downloaded again and matched `SHA256SUMS`. All five portable archives identify version `0.7.0`, their intended platform/architecture and the exact release commit. The public macOS ZIP and DMG match the signed candidate verified locally: Developer ID signature, notarization ticket, Gatekeeper acceptance, DMG installation copy, and offline native chemistry with Python, uv and the checkout unavailable. No Windows/Linux GUI update or later-version in-app upgrade is claimed by those package checks.

The live homepage displays **v0.7.0** and retains its working latest-release link and promotion video at desktop and mobile widths. Documentation and the illustrated changelog are deployed at [reshiki.com](https://reshiki.com/). No unsupported interchange or chemistry feature is promoted to validated support by packaging; the [0.7 release notes](changes-0.7.md) retain those limits.

## Legacy reference expectations

The [first tagged attempt](https://github.com/Ameyanagi/ReShiki/actions/runs/35860941693) built all five packages and passed macOS signing and notarization, but did not publish. Live comparisons caught two intentional differences from the original Python worker: exported dummy atoms are hidden, and imported ChemDraw groups retain their explicit left/right alignment.

The test correction keeps the original worker independent. A separate comparator checks the exact hidden-dummy attributes and compares every remaining drawing attribute, label, coordinate and connection; binary CDX uses the original Python codec. Negative tests reject changes to elements, bond orders/endpoints, charges, positions, labels and visibility in both formats. Group alignment expectations come from the saved ChemDraw fixture. Runtime routing remains checked in a guarded subprocess; the independent output comparison runs afterward in the parent. No application code changed in this correction.

The three affected Rust suites passed locally: 20 migration, 11 response and six routing tests, with one existing subprocess entry point ignored in the main runner. Both comparator unit tests passed. The corrected candidate then passed the complete three-platform live reference suite. The unpublished tag was updated to this corrected candidate; the original tag is preserved as `release-attempt/0.7.0-1` for traceability.
