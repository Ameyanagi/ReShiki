# Development

ReShiki uses one Rust application executable for the editor, chemistry and native integrations, and Astro Starlight for documentation. Python/RDKit is an optional test reference.

## Build and run

Install Rust 1.99.0 with rustfmt and Clippy, a C/C++ compiler, and Python 3.12 for the build scripts. The repository pins this compiler and its rustfmt/Clippy components in `rust-toolchain.toml`, and CI uses the same version. On Windows, use a Visual Studio tools shell matching your Rust target.

On macOS, Cargo defaults C/C++ dependencies to Apple Clang from `/usr/bin`, so a GCC installation earlier on `PATH` does not change the compiler. Set `CC_aarch64_apple_darwin` and `CXX_aarch64_apple_darwin` (or the `x86_64_apple_darwin` equivalents for Intel) explicitly to override these repository defaults.

```sh
git clone https://github.com/Ameyanagi/ReShiki.git reshiki
cd reshiki
cargo run --locked
```

For Windows ARM, build with `--target aarch64-pc-windows-msvc` from an ARM64 Visual Studio tools shell. The application relaunches itself for bounded InChI operations; macOS clipboard and printing use separate modes of the same binary. There are no sibling runtime executables or Swift build steps. For offline Cargo builds, populate the dependency cache first and set `CARGO_NET_OFFLINE=true`.

## Native checks

```sh
python3 scripts/build_inchi_helper.py
export RESHIKI_INCHI_HELPER="$PWD/artifacts/inchi-helper/reshiki-inchi-helper"
export RESHIKI_REQUIRE_INCHI_HELPER=1
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo run --locked -- --engine-check
```

To test one library layer, select its crate. Integration tests, binaries and examples belong to the root `reshiki` package, so qualify them with `-p reshiki`:

```sh
cargo test --locked -p reshiki-chemistry
cargo test --locked -p reshiki-model
cargo test --locked -p reshiki-io
cargo test --locked -p reshiki-agent
cargo test --locked -p reshiki --test theme_reference
```

A single-package build resolves features for that package alone, so the first run may compile some dependencies again with a smaller feature set. See [crates and layering](architecture.md#crates-and-layering) for what each crate contains.

The development-only helper and fault-injection stub let library tests exercise subprocess transport independently of the GUI entry point. Keep `RESHIKI_INCHI_HELPER` set while running those tests. In PowerShell, use `python`, set `$env:RESHIKI_INCHI_HELPER = (Resolve-Path artifacts/inchi-helper/reshiki-inchi-helper.exe).Path`, and set `$env:RESHIKI_REQUIRE_INCHI_HELPER = '1'`. Required mode fails if native test binaries are missing. Default tests do not require Python or RDKit. Windows clipboard tests replace the desktop clipboard with test data.

Archive and installer checks run the installed app twice with Python, uv and the checkout unavailable. They reject separate worker executables, Python payloads and chemistry-environment creation. Release CI also runs the complete InChI framing and heap suite against the application’s `--inchi-worker` mode.

## Optional RDKit reference tests

Install [uv 0.12.3 or later](https://docs.astral.sh/uv/getting-started/installation/) and run `uv sync --locked --python 3.12`. On Windows, use `--python cpython-3.12-windows-x86_64-none`, including on ARM.

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

Pull requests run native tests and saved InChI, depiction, and aromatic-response fixtures on macOS ARM64, Linux x64, and Windows x64. Documentation-only changes skip native checks. Expected results come from pinned RDKit captures; fixture updates retain their source hashes and platform provenance.

The complete live RDKit suite runs weekly, before publishing a tagged release, or on demand. Four parallel shards per platform retain every integration target, plus workspace unit and documentation tests:

```sh
gh workflow run checks.yml --ref main -f live_reference=true
```

To refresh aromatic-fixture provenance after an unrelated worker-source change, dispatch `checks.yml` with `capture_aromatic=true`. Its three platform jobs replay the existing request corpus through the pinned Python worker, retain the historical template inputs, and require an unchanged request/response hash. Download the `aromatic-capture-*` artifacts, compare their records and source hashes, then commit the updated captures. A changed response fails this refresh path and requires a separate review; expected results are never generated by the native implementation.

Windows ARM64 and Linux ARM64 remain release targets with package and installer checks.

## Pre-commit checks

Install [Bun 1.4.2](https://bun.com/docs/installation), then run `bun install --frozen-lockfile` to install the web tools and Lefthook hooks. Use `bun run --bun lefthook install` to reinstall hooks. Python changes also need the uv development environment above.

Hooks check staged file types: Oxlint/Oxfmt for web and configuration files, Ruff/ty for Python, and Cargo fmt/Clippy/check for Rust. They do not rewrite or restage changes. Rust checks run sequentially. The repository uses LF line endings, including on Windows.

```sh
bun run lint:js
bun run format:check
uv run --locked ruff check reference scripts tests
uv run --locked ruff format --check reference scripts tests
uv run --locked ty check
```

To format, use `bun run format`, `uv run --locked ruff format reference scripts tests`, or `cargo fmt --all`.

## Documentation

```sh
bun run docs:dev
bun run docs:check
bun run docs:build
```

Edit the visual manual in `website/src/content/docs/guide/` and developer notes in `docs/*.md`. The sync script generates ignored Starlight copies of the developer notes; edit their originals. Landing page, navigation and theme live in `website/`. The production build checks local page and asset links.

The Documentation workflow checks pull requests and deploys main to [GitHub Pages](https://reshiki.com/). Pages must use **GitHub Actions** as its source. See [release builds and signing](releasing.md) for distribution setup.

## Windows development

Use the MSVC Rust toolchain with Visual Studio Build Tools and the Windows SDK. Measure performance with `cargo run --release --locked`. Windows includes WGPU with a Tiny Skia fallback and selects Tiny Skia directly when adapter discovery finds only CPU rendering or no adapters. `ICED_BACKEND` overrides the automatic choice; `--graphics-info` reports adapter discovery. Clipboard, printing and Office integration are Rust code linked into the app. See the [Windows guide](windows.md).
