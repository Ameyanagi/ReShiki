# ReShiki 0.16.0 release validation

This record lists the checks run on the 0.16.0 release preparation and the
checks that remain before the tag.
[Release notes](../changes-0.16.md) · [Release procedure](../releasing.md).

## Release preparation

The release preparation pull request sets the version to 0.16.0 in
`Cargo.toml` (`[package]` and `[workspace.package]`, which the Python package
follows) and `Cargo.lock`, moves the 52 release-note fragments into the
[release notes](../changes-0.16.md) with
`scripts/assemble_changelog.py --release 0.16.0`, adds the GitHub release
highlights in `.github/release-notes/v0.16.0.md` and updates the install
guide's file compatibility section. The npm packages take their version from
`Cargo.toml` when the tag publishes them, so `packages/editor/package.json`
and `packages/headless/package.json` are unchanged. The source is `main` at
`f25ab7eb`, the merge of
[PR #1234](https://github.com/Ameyanagi/ReShiki/pull/1234), 57 first-parent
merges after v0.15.0.

`VERSION` is 39 in `crates/model/src/document.rs`. `Document::file_version`
saves each drawing at the version of the newest feature it uses: 39 for a
property caption with a **Name** line or a custom name, 38 for any other
property caption linked to its molecule, 37 for an atom with more than two
radical electrons, 36 for an atom in a stereo group, 35 for an arrow with
interior nodes or a drawing style whose crossing clearance is not 1.1 pt, 34
for a two-sided residue group, 33 for a generated 3D model, model atom labels
or a plain XYZ model, 32 for ORTEP settings and 31 for a custom reaction-label
gap. Any other drawing is saved as version 30. ReShiki 0.15.0 reads up to
version 37, 0.14.0 up to 34 and 0.13.1 up to 30.

NMR shift labels from Python, the command line and MCP are atom numbers, which
every version since 30 stores, so they do not raise the version. A PNG's
editable drawing moved after the image data
([PR #1192](https://github.com/Ameyanagi/ReShiki/pull/1192)); 0.15.0's
`payload::read_png` walks every chunk until `IEND`, so it still finds it.

## Automated checks

These ran on the release preparation branch at `f25ab7eb` plus the version
change, on macOS arm64: the Rust checks on an Apple M4 Mac and the Python and
documentation checks on an Apple M1 Mac. The Rust tests ran with
`RESHIKI_INCHI_HELPER` and `RESHIKI_NAMING_HELPER` set to the debug build,
`RESHIKI_REQUIRE_INCHI_HELPER=1` and `RESHIKI_DATA_DIR` set to an empty
temporary folder:

| Check                      | Command                                                                                                                              | Result                                                                        |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------- |
| Build                      | `cargo build --locked --bin reshiki`                                                                                                 | Passed                                                                        |
| Version                    | `reshiki --cli --version` from the debug build                                                                                       | `reshiki 0.16.0`                                                              |
| Tag check                  | `scripts/build_release.py --tag v0.16.0 --check-tag-only`                                                                            | Passed; `v0.15.0` is refused                                                  |
| Application library tests  | `cargo test --locked -p reshiki --lib`                                                                                               | 514 passed, 14 ignored                                                        |
| Integration tests          | `cargo test --locked -p reshiki --test it -- theme_reference:: windows_version_metadata:: mcp_stdio:: mcp_tools:: headless_parity::` | 31 passed (1, 4, 14, 5 and 7)                                                 |
| Colour reference           | `cargo run --locked --example theme_colors`, then `bun fmt`                                                                          | Regenerated data matches main except `reshiki_version`                        |
| Rust format and lint       | `cargo fmt --all --check`, `cargo clippy --workspace --lib --bins --all-features --locked -- -D warnings`                            | Passed                                                                        |
| Python                     | `uv run --locked python -m unittest discover -s tests -p 'test_*.py'`                                                                | 288 passed, 12 skipped                                                        |
| Release notes              | `scripts/assemble_changelog.py --check`                                                                                              | 0 fragments left                                                              |
| JavaScript format and lint | `bun run format:check`, `bun run lint:js`                                                                                            | Passed                                                                        |
| Documentation              | `bun run docs:check`                                                                                                                 | 0 errors, 0 warnings, 0 hints                                                 |
| Website and links          | `bun run docs:build`                                                                                                                 | 13,717 local links/assets, llms.txt, agent-api.txt and 1,360 colours verified |

The pull request's Checks workflow runs the native, browser (wasm32),
Python and documentation jobs again; its results are on the pull request.

## Manual checks before the tag

The maintainer runs these on a manual **Release builds** run of the merged
`main` commit, or on its packages, and records the results here:

- [ ] **Release build:** a manual **Release builds** run passes on all six
      platforms and does not publish; see [Release build](#release-build).
- [ ] **Live reference suite:** the complete live RDKit reference tests pass
      on macOS arm64, Linux x64 and Windows x64; see
      [Live RDKit reference suite](#live-rdkit-reference-suite).
- [ ] **Focus after closing reviews:** in the release build, run Clean up from
      the toolbar and close its ▾ menu with Esc, and close 3D preview, atom
      label, Name import preview, Structure import, reaction mapping proposal
      and assistant review with Enter and separately with Esc (Apply/Discard
      where those keys are unavailable). After each close, Delete or Cmd+Z
      acts on the drawing without a click. **Pending.**
- [ ] **macOS app (Apple Silicon and Intel):** the DMG opens, the app reports
      0.16.0 and moves an existing data folder to `~/.reshiki/`, opens
      **Settings** with Cmd+, and copies a drawing on a white background at
      300 dpi, shows properties with a **Name** line under a molecule and saves
      and reopens the drawing, opens a MOL file as itself and keeps it as MOL
      on **Save**. **Pending.**
- [ ] **Windows x64:** setup installs over 0.15.0, the app starts, keeps its
      data folder, names a molecule and edits an embedded drawing in
      PowerPoint. Windows ARM64 is covered by the CI package checks only.
      Downloads remain unsigned. **Pending.**
- [ ] **Linux (Arch Linux x64 and Ubuntu arm64):** the portable archive starts,
      moves an existing data folder to `~/.reshiki/`, opens a drawing and an SD
      file, and names a molecule. **Pending.**
- [ ] **Browser demo:** a demo built with `scripts/build_wasm.sh` and
      `scripts/package_web_demo.sh` loads in Chrome, Safari and Firefox,
      draws, and converts a name with **Import → Name**.
- [ ] **npm packages:** the packages built by the release workflow pass their
      tests; after publication `@ameyanagi/reshiki@0.16.0` is `latest`, and
      `@ameyanagi/reshiki-headless@0.16.0`, the first version of that package,
      is published (see
      [Configure npm publishing](../releasing.md#configure-npm-publishing)).
- [ ] **PyPI:** after publication `uv add reshiki` installs 0.16.0 on each
      platform, and `Molecule.from_smiles("CCO").predict_nmr("13C")` returns
      rows.
- [x] **Compatibility:** drawings saved by this branch at versions 30, 38 and
      39 checked against the published 0.15.0; see [Compatibility](#compatibility).

## Release build

Manual **Release builds**
[run 44269689222](https://github.com/Ameyanagi/ReShiki/actions/runs/44269689222)
at `f25ab7eb`: **running**.

## Live RDKit reference suite

The complete live reference tests gate publication in `release.yml`.
**Checks** with `live_reference=true`
[run 44269684105](https://github.com/Ameyanagi/ReShiki/actions/runs/44269684105)
at `f25ab7eb` runs them on macOS arm64, Linux x64 and Windows x64: **running**.

## Compatibility

Checked on macOS arm64 with `reshiki --cli convert … --to smiles`. The `.rsk`
files were written by this branch's `Document::file_json`, the code the app
saves with, from phenol with the captions below; each was read by the debug
build of this branch and by the published 0.15.0 macOS arm64 download
(`reshiki-0.15.0-macos-arm64.zip`, SHA-256 checked against `SHA256SUMS`):

| Drawing                                             | Saved as | 0.16.0 | 0.15.0                                                                                          |
| --------------------------------------------------- | -------- | ------ | ----------------------------------------------------------------------------------------------- |
| No caption                                          | 30       | Opens  | Opens                                                                                           |
| Property caption with Mol. Wt. and Formula          | 38       | Opens  | Refused: "This drawing was made with a newer version of ReShiki (document version 38). Update…" |
| Property caption with a **Name** line               | 39       | Opens  | Refused: "This drawing was made with a newer version of ReShiki (document version 39). Update…" |
| Property caption with the custom name `Compound 3a` | 39       | Opens  | Refused: "This drawing was made with a newer version of ReShiki (document version 39). Update…" |

Both builds read the drawing as `Oc1ccccc1`. The 0.15.0 CLI cannot load a
data folder or preferences, so the data folder move is described from the code
([PR #1216](https://github.com/Ameyanagi/ReShiki/pull/1216)), not from a run.

## Build and verification

Moved from the [release notes](../changes-0.16.md).

- **Release builds.** Published builds use a new Cargo `dist` profile: the
  `release` profile with fat link-time optimization and one codegen unit. The
  macOS builds use thin link-time optimization, because the fat-LTO link needs
  more memory than the macOS arm64 runner has. The macOS arm64 executable goes
  from 81.4 MB to 72.6 MB and the portable archive from 33.8 MB to 32.7 MB.
  `scripts/build_release.py` and the Release builds workflow read the binary
  and its symbol file from `target/<target>/dist/`; `cargo build --release`,
  tests and development bundles are unchanged. Embedded catalogs are stored
  compressed, and TLS uses ring, so the build no longer needs CMake
  ([#1202](https://github.com/Ameyanagi/ReShiki/pull/1202);
  [Build locally](../releasing.md#build-locally)).
- **Linux arm64.** The Linux arm64 release build uses the `release` profile's
  settings (`lto = false`, 16 codegen units): its fat-LTO link peaked at
  18.6 GiB on a 16 GB runner. It now peaks at 2.8 GiB; the portable archive is
  36.7 MB instead of 35.8 MB
  ([#1213](https://github.com/Ameyanagi/ReShiki/pull/1213)).
- **Packages.** macOS disk images use LZMA compression and are about 40%
  smaller. Release executables and the Python package carry no symbols; the
  Release builds workflow keeps a separate symbol file for crash reports
  ([#1178](https://github.com/Ameyanagi/ReShiki/pull/1178)).
- **Nightly builds** are no longer made for Intel Macs: that build kept
  reaching its 100-minute limit. Stable releases still include Intel Mac
  downloads, and the **Nightly** update channel on an Intel Mac says so
  ([#1218](https://github.com/Ameyanagi/ReShiki/pull/1218)).
- **Release notes.** Each pull request adds its release note as one file in
  [`changes/unreleased/`](unreleased/README.md); `scripts/assemble_changelog.py`
  checks, assembles and moves them
  ([#1178](https://github.com/Ameyanagi/ReShiki/pull/1178)). It writes UTF-8
  whatever the Windows code page is, and the documentation workflow runs when
  it changes ([#1180](https://github.com/Ameyanagi/ReShiki/pull/1180)).
  [Writing and terms](../writing-style.md#release-notes) sets the sections of
  every release note, and release validation records live in
  `docs/changes/` ([#1190](https://github.com/Ameyanagi/ReShiki/pull/1190)).
- **Documentation.** The website sidebar follows a learning path; 29 review and
  validation records moved to `docs/changes/` with redirects; the README is a
  short landing page; [Build and test](../development.md) lists the
  prerequisites for each system; guide screenshots are lossless WebP (18.15 MB
  to 4.55 MB) ([#1190](https://github.com/Ameyanagi/ReShiki/pull/1190)). The
  old getting-started page redirects to the install guide, and
  `vendor/README.md` lists every vendored crate, checked by
  `scripts/check_vendor.py`
  ([#1178](https://github.com/Ameyanagi/ReShiki/pull/1178)). The ruviz note,
  which described a library ReShiki never used, is removed
  ([#1224](https://github.com/Ameyanagi/ReShiki/pull/1224)).
- **Tests.** The files under `tests/` build as one test binary, `it`, except
  `agent_api_windows_pipes`, `metafile_worker` and `picture_memory`; run one
  file's tests with `cargo test -p reshiki --test it theme_reference::`. The
  integration tests built in 37 seconds instead of 334 on an Apple Silicon
  Mac. On Windows, CI runs every ignored application test not named
  `manual_…` or `…_child` (145 renderer checks). Three stale capture tests are
  removed, and the [module index](../modules.md) is generated and checked
  ([#1201](https://github.com/Ameyanagi/ReShiki/pull/1201);
  [Opt-in renderer tests](../development.md#opt-in-renderer-tests)).
  `cargo test --release --bin reshiki` compiles again
  ([#1186](https://github.com/Ameyanagi/ReShiki/pull/1186)). The spiro Clean
  Up regression test covers every scope and orientation
  ([#1182](https://github.com/Ameyanagi/ReShiki/pull/1182)), and a worker test
  on Linux no longer fails when `/proc` returns `ESRCH`
  ([#1194](https://github.com/Ameyanagi/ReShiki/pull/1194)).
- **Shared test data.** The PNG preview and property rounding are in
  `reshiki-io` (`preview::png`, `Analysis::rounded`), and
  `tests/fixtures/agent-api/parity.json` holds the answers the command line,
  Python and JavaScript must all give
  ([#1188](https://github.com/Ameyanagi/ReShiki/pull/1188)).
- **CI.** The workspace tests compile once per platform and the partition jobs
  run the archive. Pull request checks keep their build cache, while release,
  nightly and corpus builds no longer save caches. The InChI helper builds for
  the host, two naming-worker tests run at a time, nextest stops a test after
  15 minutes, the **Changed fixture rows** job lists gained and lost rows, and
  the macOS Python and geometry checks share the macOS lint job. Clippy runs
  in a pre-push hook ([#1187](https://github.com/Ameyanagi/ReShiki/pull/1187),
  [#1201](https://github.com/Ameyanagi/ReShiki/pull/1201);
  [CI](../development.md#ci)). The live reference shards are rebalanced from
  measured durations, cached, and run in their own concurrency group
  ([#1174](https://github.com/Ameyanagi/ReShiki/pull/1174)).
- **0.15.0 release validation.** The preparation record of 0.15.0.
  [Validation record](release-0.15.0-validation.md) ·
  [PR #1170](https://github.com/Ameyanagi/ReShiki/pull/1170).
- **Release preparation.** Update the version, the release notes and the
  documentation for 0.16.0.
