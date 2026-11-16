# Development

ReShiki is one Rust application executable for the editor, chemistry and
native integrations, with Astro Starlight for this website. Python and RDKit
are only an optional test reference.

## Prerequisites

| Tool              | macOS                                                            | Windows                                                                             | Linux (Ubuntu or Debian package names)                                                                                                             |
| ----------------- | ---------------------------------------------------------------- | ----------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust              | [rustup](https://rustup.rs/)                                     | [rustup](https://rustup.rs/) with the MSVC toolchain                                | [rustup](https://rustup.rs/)                                                                                                                       |
| C/C++ compiler    | Xcode Command Line Tools (`xcode-select --install`)              | Visual Studio Build Tools with **Desktop development with C++** and the Windows SDK | `build-essential pkg-config`                                                                                                                       |
| System libraries  | None                                                             | None                                                                                | `libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev libxcursor-dev libxinerama-dev libgl1-mesa-dev libfontconfig1-dev libudev-dev` |
| Python and uv     | Python 3.12 and [uv](https://docs.astral.sh/uv/) 0.12.3 or later | Python 3.12 (run as `python`) and uv 0.12.3 or later                                | Python 3.12 and uv 0.12.3 or later                                                                                                                 |
| Hooks and website | [Bun 1.4.4](https://bun.com/docs/installation)                   | Bun 1.4.4                                                                           | Bun 1.4.4                                                                                                                                          |
| Test runner       | [cargo-nextest](https://nexte.st/) 0.9.150, optional             | cargo-nextest 0.9.150, optional                                                     | cargo-nextest 0.9.150, optional                                                                                                                    |

`rust-toolchain.toml` pins Rust 1.99.1 with rustfmt and Clippy; rustup installs
it on the first build, and CI uses the same version. The Linux list is the one
the CI images install. On Windows, run every command from a Visual Studio tools
shell that matches your Rust target: x64, or ARM64 for
`--target aarch64-pc-windows-msvc`.

## First build

```sh
git clone https://github.com/Ameyanagi/ReShiki.git reshiki
cd reshiki
cargo run --locked
```

The application relaunches itself for bounded InChI, geometry and naming
work, and for some platform services; see [worker modes](architecture.md#worker-modes).
There are no sibling runtime executables or Swift build steps. For offline
builds, populate the Cargo cache first and set `CARGO_NET_OFFLINE=true`.
Measure performance with `cargo run --release --locked`; debug builds do much
more work when drawing.

## Test environment

Tests that start helper processes need these variables. Set them in each shell
after you build the helpers (see [Run the checks](#run-the-checks)). Use
absolute paths. If you set `CARGO_TARGET_DIR`, point at the binary there.

| Variable                       | sh                                                                               | PowerShell                                                                                         |
| ------------------------------ | -------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `RESHIKI_INCHI_HELPER`         | `export RESHIKI_INCHI_HELPER="$PWD/artifacts/inchi-helper/reshiki-inchi-helper"` | `$env:RESHIKI_INCHI_HELPER = (Resolve-Path artifacts/inchi-helper/reshiki-inchi-helper.exe).Path`  |
| `RESHIKI_REQUIRE_INCHI_HELPER` | `export RESHIKI_REQUIRE_INCHI_HELPER=1`                                          | `$env:RESHIKI_REQUIRE_INCHI_HELPER = '1'`                                                          |
| `RESHIKI_NAMING_HELPER`        | `export RESHIKI_NAMING_HELPER="$PWD/target/debug/reshiki"`                       | `$env:RESHIKI_NAMING_HELPER = (Resolve-Path target/debug/reshiki.exe).Path`                        |
| `RESHIKI_DATA_DIR`             | `export RESHIKI_DATA_DIR="$(mktemp -d)"`                                         | `$env:RESHIKI_DATA_DIR = (New-Item -ItemType Directory (Join-Path $env:TEMP (New-Guid))).FullName` |

- `RESHIKI_INCHI_HELPER` is the development InChI helper. With
  `RESHIKI_REQUIRE_INCHI_HELPER=1`, tests fail instead of skipping when a
  helper is missing.
- `RESHIKI_NAMING_HELPER` is the built app, which naming tests start in its
  `--naming-worker` mode. Rebuild it after you change naming or worker code,
  because the tests run it as built.
- `RESHIKI_DATA_DIR` keeps app-level tests away from your own recovery drafts
  and templates. Use a new, empty folder for each run.

Default tests do not need Python or RDKit. Windows clipboard tests replace the
desktop clipboard with test data.

## Run the checks

Build the helpers, then lint:

```sh
uv run --no-project python scripts/build_inchi_helper.py
cargo build --locked --bin reshiki
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Run the tests as CI does, with cargo-nextest. The targets left out of the
nextest run have custom harnesses or share one baseline in one process, so they
run under `cargo test`:

```sh
cargo nextest run --workspace --locked --no-default-features --profile ci \
  -E 'not binary_id(reshiki::metafile_worker) & not binary_id(reshiki-macos::native_print) & not (package(reshiki) & kind(lib) & test(/^naming::s2n_harness::/))'
cargo test --locked --no-default-features -p reshiki --lib naming::s2n_harness::
cargo test --locked --no-default-features --test metafile_worker
cargo test --locked --no-default-features -p reshiki-macos --test native_print
cargo test --workspace --locked --no-default-features --doc
cargo run --locked -- --engine-check
```

PowerShell continues a line with a backtick instead of a backslash:

```powershell
cargo nextest run --workspace --locked --no-default-features --profile ci `
  -E 'not binary_id(reshiki::metafile_worker) & not binary_id(reshiki-macos::native_print) & not (package(reshiki) & kind(lib) & test(/^naming::s2n_harness::/))'
```

The other commands are the same in both shells.

Without cargo-nextest, `cargo test --workspace --locked` runs the same tests in
one command. `.config/nextest.toml` runs at most two of the tests that start
the naming worker at a time and stops any test that runs for 15 minutes.

To test one library layer, select its crate. Integration tests, binaries and
examples belong to the root `reshiki` package, so qualify them with
`-p reshiki`. The files under `tests/` are modules of one test binary, `it`, so
that the application library links once and not once per file. Select one
file's tests by its module name:

```sh
cargo test --locked -p reshiki-model
cargo test --locked -p reshiki --test it theme_reference::
```

A new file under `tests/` needs a `mod` line in `tests/it.rs`; a test there
fails while one is missing. Three files stay separate targets in `Cargo.toml`:
`metafile_worker` has its own `main`, `picture_memory` its own global
allocator, and `release.yml` builds `agent_api_windows_pipes` alone in the
release profile.

The agent goldens under `tests/fixtures/agent-contract/` and
`tests/fixtures/agent-skill/` change only with an intentional contract change;
the README in each folder says how to regenerate them. The MCP transcripts
under `tests/fixtures/mcp/` and `crates/mcp/tests/transcripts/` are edited by
hand to the new expected lines. `tests/fixtures/agent-api/parity.json` holds
one molecule and one reaction with the answers the command line
(`tests/cli.rs`), the Python package (`crates/python/tests/test_parity.py`) and
the JavaScript package (`packages/headless/test/parity.test.ts`) must all give.
Rewrite it from a built binary:

```sh
uv run --locked python scripts/agent_parity_fixture.py target/debug/reshiki
```

A single-package build resolves features for that package alone, so the first
run may compile some dependencies again. `reshiki-metafile` is not a default
member, so plain `cargo test` skips it. See [crates and layering](architecture.md#crates-and-layering)
for what each crate contains.

The naming project's five crates now live in [Ameyanagi/nazuchem](https://github.com/Ameyanagi/nazuchem). ReShiki uses `nazuchem-ir`, `nazuchem-kernel`, `nazuchem-n2s` and `nazuchem-s2n` as git dependencies at the nazuchem revision pinned in `Cargo.toml`; it does not use the facade crate `nazuchem`. Run the engine unit tests and oracle gates in that repository. ReShiki's `cargo test --workspace --locked` covers the adapters, naming integration tests and worker response snapshot; it does not run dependency unit tests. Use the [structure-to-name harness](structure-to-name-harness.md#run) for focused ReShiki naming checks.

The [structure-to-name harness](structure-to-name-harness.md) measures naming coverage and openclatura parity with an in-process OPSIN round trip, without a helper executable. Its fast subset runs normally; the full corpus is opt-in and runs nightly in CI.

Archive and installer checks run the installed app twice with Python, uv and the checkout unavailable. They reject separate worker executables, Python payloads and chemistry-environment creation. Release CI also runs the complete InChI framing and heap suite against the application’s `--inchi-worker` mode.

## Pre-commit checks

Run `bun install --frozen-lockfile` to install the web tools and the Lefthook
hooks; `bun run --bun lefthook install` reinstalls them. Python changes also
need `uv sync --locked`.

Pre-commit hooks check staged file types: Oxlint and `bun fmt` for web and
configuration files, Ruff and ty for Python, and Cargo fmt for Rust. A pre-push
hook runs Clippy when the push changes Rust or Cargo files. Clippy type-checks
libraries and binaries; CI lints tests and examples too. Hooks do not rewrite or
restage changes. The repository uses LF line endings, including on Windows.

```sh
bun run lint:js
bun run format:check
uv run --locked ruff check reference scripts tests
uv run --locked ruff format --check reference scripts tests
uv run --locked ty check
uv run --locked python -m unittest discover -s tests -p 'test_*.py'
```

To format, use `bun run format`, `uv run --locked ruff format reference scripts tests`
or `cargo fmt --all`.

## Troubleshooting

- **"Naming integration tests require RESHIKI_NAMING_HELPER…"**: run
  `cargo build --locked --bin reshiki` and set `RESHIKI_NAMING_HELPER` to the
  binary with an absolute path.
- **A naming test still fails after a fix**: rebuild `reshiki`; the tests run
  the helper as built.
- **macOS uses the wrong C compiler**: Cargo uses Apple Clang from `/usr/bin`,
  even with GCC earlier on `PATH`. To override it, set
  `CC_aarch64_apple_darwin` and `CXX_aarch64_apple_darwin` (or the
  `x86_64_apple_darwin` equivalents on Intel).
- **Windows cannot find `link.exe` or the SDK**: open a Visual Studio tools
  shell for your target architecture and run Cargo from there.
- **Linux build fails in a `-sys` crate**: install the system libraries from
  [Prerequisites](#prerequisites); `pkg-config` must find them.
- **`--locked` fails**: `Cargo.lock` does not match `Cargo.toml`. With a local
  nazuchem patch, omit `--locked` (see [Working on nazuchem](#working-on-nazuchem)).
- **A renderer test is skipped**: tests that need a real renderer are
  `#[ignore]`; see [Opt-in renderer tests](#opt-in-renderer-tests).

## Working on nazuchem

To test unmerged engine changes, check out nazuchem beside ReShiki and add this temporary patch to ReShiki's root `Cargo.toml`:

```toml
[patch."https://github.com/Ameyanagi/nazuchem"]
nazuchem-ir = { path = "../nazuchem/crates/nazuchem-ir" }
nazuchem-kernel = { path = "../nazuchem/crates/nazuchem-kernel" }
nazuchem-n2s = { path = "../nazuchem/crates/nazuchem-n2s" }
nazuchem-s2n = { path = "../nazuchem/crates/nazuchem-s2n" }
```

Patch all four crates together. A partial patch can resolve two copies of `nazuchem-kernel` and cause type mismatches between the local and git crates. `--locked` fails while the patch changes dependency resolution, so omit it for local patch builds and tests. Keep the patch and its `Cargo.lock` changes uncommitted. Remove the patch and undo its lockfile changes before updating the pin.

After upstream CI is green at the selected revision, change all four `rev` values in `[workspace.dependencies]` to that commit, then update the lockfile:

```sh
cargo update -p nazuchem-ir -p nazuchem-kernel -p nazuchem-n2s -p nazuchem-s2n
```

As part of each pin bump, review `crates/model/src/molecule_names/ir.rs`, adapted
from nazuchem-s2n's `src/ir_input.rs` at the pinned revision. Changes to upstream
IR preparation semantics require a manual re-sync of this adapter. The worker
snapshot deliberately rejects IR versions other than 1; review its version guard
when changing the IR contract.

Then replay the [worker response snapshot](structure-to-name-harness.md#worker-response-snapshot). It calls the worker's request handler in-process, so it checks the engines' output byte-for-byte without the helper processes:

```sh
cargo test --locked --lib -p reshiki naming::s2n_harness::worker_snapshot::s2n_worker_snapshot -- --exact --nocapture
```

The other naming tests do start helper processes; run them as CI does, with `RESHIKI_INCHI_HELPER` and `RESHIKI_NAMING_HELPER` set to the rebuilt `target/debug/reshiki` (with `.exe` on Windows) and `RESHIKI_REQUIRE_INCHI_HELPER=1`.

## Opt-in renderer tests

Tests that need a real renderer or a baseline folder are marked `#[ignore]`. Run them with `-- --ignored`, or with `-- --include-ignored` to run the ordinary tests too. Add `--test-threads=1` for renderer pixel tests. On macOS the only headless renderer is WGPU on Metal; Tiny Skia is built only on Windows.

| Variable                | Used by                                                                          | Meaning                                                                                                                                                                                                                                                |
| ----------------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `RESHIKI_DATA_DIR`      | View parity, UI snapshots and other app-level renderer tests                     | The app's data folder: `App::new` reads recovery drafts and the template library from it. Point it at a new, empty folder before each run, so your own data cannot change the result.                                                                  |
| `RESHIKI_UI_QA_DIR`     | `manual_ui_layout_snapshots`                                                     | Folder for PNG snapshots of key UI states at 1280 and 1040 px. Without it, the test writes to `reshiki-ui-qa` in the temporary folder.                                                                                                                 |
| `RESHIKI_UI_QA_SCALE`   | `manual_ui_layout_snapshots`, `manual_settings_snapshots`                        | Pixel density of the snapshots, such as `2` for 2× images. Without it, they are 1×.                                                                                                                                                                    |
| `RESHIKI_VIEW_PARITY`   | `manual_views_match_captured_baseline`, `manual_entries_match_captured_baseline` | Baseline folder for view and right-click menu parity. `RESHIKI_VIEW_PARITY_CAPTURE=1` writes the baseline; otherwise the test compares. `RESHIKI_VIEW_PARITY_ONLY` limits a capture to a case-name prefix during development; compare mode refuses it. |
| `RESHIKI_CANVAS_PIXELS` | Canvas, paper-layer, text-placement and icon pixel baselines                     | Baseline folder. `RESHIKI_CANVAS_CAPTURE_BASELINE=1` writes the baseline; otherwise the test compares.                                                                                                                                                 |
| `RESHIKI_PERF_RENDERER` | Pixel, view-parity and canvas performance tests                                  | Headless backend: `wgpu`, or `tiny-skia` on Windows. Without it, the default backend runs.                                                                                                                                                             |
| `RESHIKI_MODEL_PARITY`  | `reshiki-model` captured-baseline tests                                          | Baseline folder for model output. `RESHIKI_MODEL_CAPTURE_BASELINE=1` writes it.                                                                                                                                                                        |

Capture a baseline on the parent commit, then compare on your change:

```sh
export RESHIKI_DATA_DIR="$PWD/artifacts/test-data"
rm -rf "$RESHIKI_DATA_DIR" && mkdir -p "$RESHIKI_DATA_DIR"
RESHIKI_VIEW_PARITY="$PWD/artifacts/view-parity" RESHIKI_VIEW_PARITY_CAPTURE=1 \
  cargo test --locked --bin reshiki manual_views_match_captured_baseline -- --ignored --nocapture
```

For the comparison, empty `RESHIKI_DATA_DIR` again and run the same command without `RESHIKI_VIEW_PARITY_CAPTURE`. Set `RESHIKI_NAMING_HELPER` and `RESHIKI_INCHI_HELPER` the same way for both runs: with the naming helper set, view parity also registers the cases that need chemical names.

On Windows, CI runs every ignored test of the application binary whose name does not start with `manual_` or end in `_child`, and the accessibility tests of the root library. A new renderer test in the application runs there without a workflow change:

```sh
cargo nextest run --locked --no-default-features --lib --bin reshiki --profile ci --run-ignored ignored-only --test-threads 2 \
  -E '(binary_id(reshiki::bin/reshiki) & not test(/(^|::)manual_[^:]+$/) & not test(/_child$/)) | (package(reshiki) & kind(lib) & test(/^accessibility::tests::/))'
```

Start a test's name with `manual_` when it is a tool and not a check: a benchmark or profile, a baseline comparison, or a capture that needs its own folder or variables. Nothing in CI runs a `manual_` test, so keep its assertions few.

End a test's name with `_child` when another test starts it in a new process and sets its environment, as `define_group_child` is started. CI runs it only through that parent test.

## Optional RDKit reference tests

Install [uv 0.12.3 or later](https://docs.astral.sh/uv/getting-started/installation/) (`required-version` in `pyproject.toml`; CI uses 0.13.5) and run `uv sync --locked --python 3.12.15`. On Windows, use `--python cpython-3.12.15-windows-x86_64-none`, including on ARM.

Run reference/source HTTPS downloads through the locked environment so they use
the updated certifi CA bundle. Offline packaging does not need certifi. The
[dated security review](changes/security-dependency-audit.md) records runtime
requirements and accepted exceptions.

The reference is pinned to RDKit 2026.9.1 (`Release_2026_09_1`). RDKit 2026.09.2 is tagged but its wheel is not on PyPI yet; the pin moves to 2026.9.2 when it ships. Reference runs use RDKit's legacy ring symmetrization, which ReShiki keeps: every live oracle calls `Chem.SetUseLegacyRingFinding(True)` before any RDKit work. [`reference/README.md`](../reference/README.md#deliberate-differences) lists where ReShiki deliberately differs from the pinned RDKit.

The pinned RDKit reference provides wheels for Apple Silicon macOS, Windows x64 and Linux x64/ARM64, but not Intel macOS. Intel Mac development and packaging use the native checks above; run optional reference comparisons on a supported platform. The distributed Intel app needs no Python or RDKit.

Live layout comparisons on Linux and Windows need fresh references for their installed math libraries. Setup downloads the pinned RDKit source and verified Boost headers; it only builds test tools.

On Linux:

```sh
uv run --locked python scripts/setup_linux_depict_reference.py
source artifacts/depict-live/environment.sh
```

On Windows, run setup from an **x64** Visual Studio tools shell:

```powershell
uv run --locked python scripts/setup_windows_depict_reference.py
```

Then, in the tools shell matching your Rust target, load the reference paths:

```powershell
. ./artifacts/depict-live-windows/environment.ps1
```

Run the comparisons on a supported reference platform:

```sh
cargo test --workspace --locked --features rdkit-reference --no-fail-fast
uv run --locked python -m unittest discover -s tests -p 'test_*.py'
uv run --locked python -m unittest discover -s reference -p 'test_*.py'
uv run --locked python scripts/check_reference_dependencies.py
```

The `rdkit-reference` feature enables `PythonEngine` and independent differential tests. It is disabled in normal builds. Tests use the checkout's `.venv`; `RESHIKI_REFERENCE_PYTHON` selects another prepared interpreter. Windows ARM uses x64 reference tools under emulation while the app and Rust tests remain native ARM64.

Everything that needs or captures from the pinned Python/RDKit reference lives in [`reference/`](../reference/README.md). `reference/engine/` is the Python worker. Each `reference/<name>.rs` is an `rdkit-reference` integration target, declared in `Cargo.toml` with an explicit `path`. The `*_reference.py`, `build_*_oracle.py` and `*.cpp` files are oracles and native observers, `reference/test_*.py` are the Python reference tests, and the Markdown notes record how fixtures were captured. Fixtures stay in `tests/fixtures/`, and `assets/drawing_style.json` holds the drawing style shared with the application.

## CI

Pull requests run native tests and saved InChI, depiction, and aromatic-response fixtures on Linux x64 and Windows x64. The macOS ARM64 legs run only on stacked `merge/*` pull requests, pull requests labelled `full-ci`, `main`, nightly, release and manual runs (`scripts/ci_scope.py`). Documentation-only changes skip native checks. Expected results come from pinned RDKit captures; fixture updates retain their source hashes and platform provenance.

On a pull request, the summary of the **Changed fixture rows** job lists the rows that each changed file under a `tests/fixtures/` folder gains and loses (`scripts/baseline_summary.py`). Read it when you review a fixture or baseline update. To print the same list locally, run `python3 scripts/baseline_summary.py "$(git merge-base origin/main HEAD)"`.

CI runs the workspace tests with [cargo-nextest](https://nexte.st/) 0.9.150; [Run the checks](#run-the-checks) gives the same commands. On each platform the **rust build** job compiles the tests once and passes them on as a `cargo nextest archive` file. Two **rust** partition jobs run that file without compiling. The **rust extra** job runs beside them and covers the tests that nextest leaves out: the Windows renderer tests, the naming corpus, the custom harnesses and the doc tests. **Rust checks** passes only when all of these jobs pass. Each partition uploads its JUnit report as a `nextest-junit-*` artifact.

The **Browser build (wasm32)** job checks the browser build on the same pull requests: Clippy with `-D warnings` for the editor and the headless library (`reshiki-web`) for `wasm32-unknown-unknown`, with the crates and target `scripts/build_wasm.sh` builds, then `bun test` in `packages/headless` against a debug build of the headless library, and in `packages/editor`. It skips with the native checks for documentation-only changes, and only release tags build the optimised bundle. The `web` job lints `packages/editor` and `packages/headless` with oxlint on every pull request. To run the same checks locally:

```sh
cargo clippy -p reshiki-web -p reshiki --lib --bin reshiki --target wasm32-unknown-unknown --locked -- -D warnings
scripts/build_wasm.sh --test
```

The complete live RDKit suite runs weekly, before publishing a tagged release, or on demand. Four parallel shards per platform retain every integration target, plus workspace unit and documentation tests:

```sh
gh workflow run checks.yml --ref main -f live_reference=true
```

To refresh aromatic-fixture provenance after an unrelated worker-source change, dispatch `checks.yml` with `capture_aromatic=true`. Its three platform jobs replay the existing request corpus through the pinned Python worker, retain the historical template inputs, and require an unchanged request/response hash. Download the `aromatic-capture-*` artifacts, compare their records and source hashes, then commit the updated captures. A changed response fails this refresh path and requires a separate review; expected results are never generated by the native implementation.

Windows ARM64 and Linux ARM64 remain release targets with package and installer checks.

## Documentation

```sh
bun run docs:dev
bun run docs:check
bun run docs:build
```

Edit the visual manual in `website/src/content/docs/guide/` and developer notes in `docs/*.md`. The sync script generates ignored Starlight copies of the developer notes; edit their originals. It builds the unreleased changes page with `scripts/assemble_changelog.py` from the [release-note fragments](changes/unreleased/README.md), so it needs `python3` (`python` on Windows). Landing page, navigation and theme live in `website/`. The production build checks local page and asset links. Follow [writing and terms](writing-style.md) for wording, UI labels and feature names.

The [module index](modules.md) is generated from the first sentence of each Rust file's `//!` comment. After you add, move or remove a module file, or change that sentence, rewrite the page; a Python test fails while it is out of date:

```sh
python3 scripts/module_index.py
```

The Documentation workflow checks pull requests and deploys main to [GitHub Pages](https://reshiki.com/). Pages must use **GitHub Actions** as its source. See [release builds and signing](releasing.md) for distribution setup.

## UI conventions

Every floating surface (menus, popovers, palettes, dialogs) wraps its shadow in `crate::appearance::surface_shadow` (`src/appearance.rs`). On Windows it returns no shadow, because Windows can render with Tiny Skia, whose shadow pass ignores the damage clip and darkens retained pixels on partial redraws. The windows-2022 CI job runs the `app::nmr` partial-redraw test (`nmr_surfaces_partial_redraws_preserve_unchanged_pixels`), which checks the NMR surfaces.

## Windows development

Windows renderers, `--graphics-info`, the renderer benchmark and the Windows
verification scope are in [Windows development](windows.md).
