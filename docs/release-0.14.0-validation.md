# ReShiki 0.14.0 release validation

This record lists the checks run on the 0.14.0 release preparation and the
checks that remain before the tag is pushed. It does not yet record a tag run,
a publishing run or the public downloads; add them here after the release, as
the [0.13.1 record](release-0.13.1-validation.md) does.
[Release notes](changes-0.14.md) · [Release procedure](releasing.md).

## Release preparation

The release preparation pull request sets the version to 0.14.0 in
`Cargo.toml` (`[package]` and `[workspace.package]`, which the Python package
follows) and `Cargo.lock`, moves the unreleased changes into the
[release notes](changes-0.14.md), adds the GitHub release highlights in
`.github/release-notes/v0.14.0.md` and updates the install guide's file
compatibility section. The npm package takes its version from `Cargo.toml`
when the tag publishes it, so `packages/editor/package.json` is unchanged.

Native output still uses document version 30 unless a drawing needs a newer
format: 31 for a custom reaction-label gap, 32 for ORTEP settings, 33 for a
generated 3D model, model atom labels or a plain XYZ model, and 34 for a
two-sided residue group (`VERSION` is 34 in `crates/model/src/document.rs`).
ReShiki 0.13.1 reads up to version 30.

0.14.0 is the first release that publishes the `reshiki` Python package to
PyPI.

## Automated checks

These ran locally on the release preparation branch, on macOS ARM64, with
`RESHIKI_INCHI_HELPER` and `RESHIKI_NAMING_HELPER` set to the debug build and
`RESHIKI_REQUIRE_INCHI_HELPER=1`:

| Check                      | Command                                                                                                                                    | Result                                                                        |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------- |
| Version                    | `reshiki --version` and `reshiki --cli --version` from a debug build                                                                       | `reshiki 0.14.0` (before: `reshiki 0.13.1`)                                   |
| Tag check                  | `scripts/build_release.py --tag v0.14.0 --check-tag-only`                                                                                  | Passed; `v0.13.1` is refused                                                  |
| Rust format                | `cargo fmt --all --check`                                                                                                                  | Passed                                                                        |
| Rust lint                  | `cargo clippy --workspace --lib --bins --all-features --locked -- -D warnings`                                                             | Passed                                                                        |
| Browser lint               | `cargo clippy -p reshiki-web -p reshiki -p reshiki-web-worker --lib --bin reshiki --target wasm32-unknown-unknown --locked -- -D warnings` | Passed                                                                        |
| Colour reference           | `cargo run --locked --example theme_colors`, then `cargo test --locked --test theme_reference`                                             | Regenerated data matches main except `reshiki_version`; 1 passed              |
| Windows version resource   | `cargo test --locked --test windows_version_metadata`                                                                                      | 4 passed                                                                      |
| Agent API version          | `cargo test --locked --test mcp_stdio --test mcp_tools --test headless_parity`                                                             | 25 passed                                                                     |
| Release Python modules     | `python -m unittest tests.test_release tests.test_prepare_nightly tests.test_release_downloads`                                            | 28 passed                                                                     |
| JavaScript format and lint | `bun run format:check`, `bun run lint:js`                                                                                                  | Passed                                                                        |
| Documentation              | `bun run docs:check`                                                                                                                       | 0 errors, 0 warnings, 0 hints                                                 |
| Website and links          | `bun run docs:build`                                                                                                                       | 14,453 local links/assets, llms.txt, agent-api.txt and 1,360 colours verified |

The pull request's Checks workflow runs the native, browser (wasm32),
Python and documentation jobs again; its results are on the pull request.

## Manual checks before the tag

The maintainer runs these on a manual **Release builds** run of the merged
`main` commit, or on its packages, and records the results here:

- [ ] **Release build:** a manual **Release builds** run passes on all six
      platforms and does not publish.
- [ ] **Focus after closing reviews:** in the release build, close Clean up,
      3D preview, atom label, Name import preview, Structure import, reaction
      mapping proposal and assistant review with Enter and separately with Esc
      (Apply/Discard where those keys are unavailable). After each close,
      Delete or Cmd+Z acts on the drawing without a click.
- [ ] **macOS app (Apple Silicon and Intel):** the DMG opens, the app reports
      0.14.0, draws and names a molecule with its status tag, imports a salt by
      name, inserts a Fischer projection and converts it to wedges, shows a 3D
      model, updates it after an edit and saves and reopens the drawing.
- [ ] **Windows x64:** setup installs over 0.13.1, the app starts, names a
      molecule, shows a 3D model and opens a crystal file. Windows ARM64 is
      covered by the CI package checks only. Downloads remain unsigned.
- [ ] **Linux (Arch Linux x64 and Ubuntu arm64):** the portable archive starts,
      opens a drawing and names a molecule.
- [ ] **Browser demo:** a demo built with `scripts/build_wasm.sh` and
      `scripts/package_web_demo.sh` loads in Chrome, Safari and Firefox,
      draws with touch, saves back to the opened file in Chrome, converts a
      name with **Import → Name** and predicts NMR.
- [ ] **npm package:** the package built by the release workflow passes its
      tests; after publication `@ameyanagi/reshiki@0.14.0` is `latest`.
- [ ] **PyPI:** the pending trusted publisher for `reshiki` exists on pypi.org;
      after publication `uv add reshiki` installs 0.14.0 on each platform.
- [ ] **Compatibility:** a plain drawing saved by 0.14.0 opens in 0.13.1; a
      drawing with a generated 3D model saved by 0.14.0 is refused by 0.13.1
      with the request to update; a drawing saved by 0.13.1 opens in 0.14.0.

## After the release

Record the tag commit, the publishing run, the public download checksums,
the macOS signing checks, the npm and PyPI versions, as in the
[0.13.1 record](release-0.13.1-validation.md).
