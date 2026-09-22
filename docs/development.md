# Development

ReShiki uses Rust for the editor and chemistry, a standalone native InChI helper, and Astro Starlight for documentation. Python/RDKit is an optional test reference.

## Build and run

Install Rust 1.95 with rustfmt and Clippy, a C/C++ compiler, and Python 3.12 for the build scripts. On Windows, use a Visual Studio tools shell matching your Rust target.

```sh
git clone https://github.com/Ameyanagi/ReShiki.git reshiki
cd reshiki
python3 scripts/build_inchi_helper.py --fetch-source
export RESHIKI_INCHI_HELPER="$PWD/artifacts/inchi-helper/reshiki-inchi-helper"
cargo run --locked
```

In PowerShell, use `python` instead of `python3` and replace the `export` line with:

```powershell
$env:RESHIKI_INCHI_HELPER = (Resolve-Path artifacts/inchi-helper/reshiki-inchi-helper.exe).Path
```

For Windows ARM, build the helper with `--target aarch64-pc-windows-msvc` from an ARM64 Visual Studio tools shell. The app and helper use the same architecture.

The helper build verifies the pinned source and records compiler, repair and binary hashes. Use `--source /path/to/INCHI-1-SRC` for an offline build. Packaged apps find the helper beside their executable; source builds use the absolute path above.

## Native checks

```sh
export RESHIKI_REQUIRE_INCHI_HELPER=1
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo run --locked -- --engine-check
```

Keep `RESHIKI_INCHI_HELPER` set. In PowerShell, use `$env:RESHIKI_REQUIRE_INCHI_HELPER = '1'`. Required mode fails if native test binaries are missing. Default tests do not require Python or RDKit. Windows clipboard tests replace the desktop clipboard with test data.

Archive and installer checks run the installed app twice with Python, uv and the checkout unavailable. They reject worker payloads and chemistry-environment creation.

## Optional RDKit reference tests

Install [uv 0.12.3 or later](https://docs.astral.sh/uv/getting-started/installation/) and run `uv sync --locked --python 3.12`. On Windows, use `--python cpython-3.12-windows-x86_64-none`, including on ARM.

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

Run the comparisons on any platform:

```sh
cargo test --workspace --locked --features rdkit-reference --no-fail-fast
uv run --locked python -m unittest discover -s tests -p 'test_*.py'
uv run --locked python scripts/check_reference_dependencies.py
```

The `rdkit-reference` feature enables `PythonEngine` and independent differential tests. It is disabled in normal builds. Tests use the checkout's `.venv`; `RESHIKI_REFERENCE_PYTHON` selects another prepared interpreter. Windows ARM uses x64 reference tools under emulation while the app and Rust tests remain native ARM64.

## CI

Pull requests run native tests and saved InChI, depiction, and aromatic-response fixtures on macOS ARM64, Linux x64, and Windows x64. Documentation-only changes skip native checks. Expected results come from pinned RDKit captures; fixture updates retain their source hashes and platform provenance.

The complete live RDKit suite runs weekly, before publishing a tagged release, or on demand. Four parallel shards per platform retain every integration target, plus workspace unit and documentation tests:

```sh
gh workflow run checks.yml --ref main -f live_reference=true
```

Windows ARM64 and Linux ARM64 remain release targets with package and installer checks.

## Pre-commit checks

Install [Bun 1.4.2](https://bun.com/docs/installation), then run `bun install --frozen-lockfile` to install the web tools and Lefthook hooks. Use `bun run --bun lefthook install` to reinstall hooks. Python changes also need the uv development environment above.

Hooks check staged file types: Oxlint/Oxfmt for web and configuration files, Ruff/ty for Python, and Cargo fmt/Clippy/check for Rust. They do not rewrite or restage changes. Rust checks run sequentially. The repository uses LF line endings, including on Windows.

```sh
bun run lint:js
bun run format:check
uv run --locked ruff check engine scripts tests
uv run --locked ruff format --check engine scripts tests
uv run --locked ty check
```

To format, use `bun run format`, `uv run --locked ruff format engine scripts tests`, or `cargo fmt --all`.

## Documentation

```sh
bun run docs:dev
bun run docs:check
bun run docs:build
```

Edit the visual manual in `website/src/content/docs/guide/` and developer notes in `docs/*.md`. The sync script generates ignored Starlight copies of the developer notes; edit their originals. Landing page, navigation and theme live in `website/`. The production build checks local page and asset links.

The Documentation workflow checks pull requests and deploys main to [GitHub Pages](https://reshiki.com/). Pages must use **GitHub Actions** as its source. See [release builds and signing](releasing.md) for distribution setup.

## Windows development

Use the MSVC Rust toolchain with Visual Studio Build Tools and the Windows SDK. Measure performance with `cargo run --release --locked`. Windows uses Iced's Tiny Skia renderer to avoid costly GPU emulation; clipboard, printing and Office integration are Rust code linked into the app. See the [Windows guide](windows.md).
