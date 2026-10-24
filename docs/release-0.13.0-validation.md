# ReShiki 0.13.0 release validation

ReShiki 0.13.0 is planned for October 25, 2026. This record lists the checks run
on the release preparation and the manual checks that remain before the tag is
pushed. It does not yet record a tag run, a publishing run or the public
downloads; add them here after the release, as the
[0.12.0 record](release-0.12.0-validation.md) does. [Release notes](changes-0.13.md).

## Release preparation

The release preparation pull request sets the version to 0.13.0 in `Cargo.toml`
and `Cargo.lock`, moves the unreleased changes into the
[release notes](changes-0.13.md), adds the GitHub release highlights in
`.github/release-notes/v0.13.0.md` and updates the install guide's file
compatibility section. The npm package takes its version from `Cargo.toml` when
the tag publishes it, so `packages/editor/package.json` is unchanged.

Drawings saved in 0.13.0 use document version 29 (`VERSION` in
`crates/model/src/document.rs`) and cannot be opened in ReShiki 0.12.x or
earlier, which read up to version 26. Older drawings still open in 0.13.0.

## Automated checks

These ran locally on the release preparation branch, on macOS ARM64:

| Check             | Command                                                                        | Result                                      |
| ----------------- | ------------------------------------------------------------------------------ | ------------------------------------------- |
| Version           | `reshiki --version` from a debug build                                         | `reshiki 0.13.0` (before: `reshiki 0.12.1`) |
| Rust format       | `cargo fmt --all --check`                                                      | Passed                                      |
| Rust lint         | `cargo clippy --workspace --lib --bins --all-features --locked -- -D warnings` | Passed                                      |
| Colour reference  | `cargo test --locked --test theme_reference`                                   | Passed                                      |
| JavaScript format | `bun run format:check`                                                         | Passed                                      |
| Documentation     | `bun run docs:check`                                                           | 0 errors, 0 warnings                        |
| Website and links | `bun run docs:build` (build, link, llms.txt and palette checks)                | Passed                                      |

The pull request's Checks workflow runs the native, browser (wasm32) and
documentation jobs again; its results are on the pull request.

## Manual checks before the tag

The maintainer runs these on a manual **Release builds** run of the merged
`main` commit, or on its packages, and records the results here:

- [ ] **Release build:** a manual **Release builds** run passes on all six
      platforms and does not publish.
- [ ] **macOS app (Apple Silicon and Intel):** the DMG opens, the app launches
      and reports 0.13.0. Draw a molecule, show it as **3D** and **Both**, switch
      styles including **Space-filling**, rotate it and export PNG, SVG and PDF.
      Open a CIF file, set **Repeat** and **Look along**, then save, close and
      reopen the drawing. Copy into PowerPoint or Word and ChemDraw.
- [ ] **Windows (x64 and ARM64):** setup installs and the app starts, shows a
      3D model and opens a crystal file. Downloads remain unsigned.
- [ ] **Linux (x64 and ARM64):** the portable archive starts and opens a
      drawing.
- [ ] **Browser demo:** a demo built with `scripts/build_wasm.sh` and
      `scripts/package_web_demo.sh` loads in Chrome, Safari and Firefox, draws,
      copies and pastes, opens and saves a file, shows a 3D model, converts a
      name with **Import → Name** and predicts NMR.
- [ ] **npm package:** the package built by the release workflow passes its
      tests; after publication `@ameyanagi/reshiki@0.13.0` is `latest`.
- [ ] **Compatibility:** a drawing saved by 0.13.0 is refused by 0.12.1 with the
      request to update, and a drawing saved by 0.12.1 opens in 0.13.0.

## After the release

Record the tag commit, the publishing run, the public download checksums and
the macOS signing checks, as in the [0.12.0 record](release-0.12.0-validation.md).
