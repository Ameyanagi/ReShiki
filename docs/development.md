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

Install [uv 0.12.3 or later](https://docs.astral.sh/uv/getting-started/installation/), then:

```sh
uv sync --locked --python 3.12
cargo test --workspace --locked --features rdkit-reference
uv run --locked python -m unittest discover -s tests -p 'test_*.py'
uv run --locked python scripts/check_reference_dependencies.py
```

The `rdkit-reference` feature enables `PythonEngine` and independent differential tests. It is disabled in normal builds. Tests use the checkout's `.venv`; `RESHIKI_REFERENCE_PYTHON` selects another prepared interpreter. Windows ARM reference tests use `uv sync --locked --python cpython-3.12-windows-x86_64-none`. This affects the test oracle, not the packaged app.

## Pre-commit checks

Install Node.js 24 LTS, then run `npm ci` to install Lefthook. Use `npx --no-install lefthook install` to reinstall hooks. Python changes also need the uv development environment above.

Hooks check staged file types: Oxlint/Oxfmt for web and configuration files, Ruff/ty for Python, and Cargo fmt/Clippy/check for Rust. They do not rewrite or restage changes. Rust checks run sequentially. The repository uses LF line endings, including on Windows.

```sh
npm run lint:js
npm run format:check
uv run --locked ruff check engine scripts tests
uv run --locked ruff format --check engine scripts tests
uv run --locked ty check
```

To format, use `npm run format`, `uv run --locked ruff format engine scripts tests`, or `cargo fmt --all`.

## Documentation

```sh
npm run docs:dev
npm run docs:check
npm run docs:build
```

Edit the visual manual in `website/src/content/docs/guide/` and developer notes in `docs/*.md`. The sync script generates ignored Starlight copies of the developer notes; edit their originals. Landing page, navigation and theme live in `website/`. The production build checks local page and asset links.

The Documentation workflow checks pull requests and deploys main to [GitHub Pages](https://reshiki.com/). Pages must use **GitHub Actions** as its source. See [release builds and signing](releasing.md) for distribution setup.

## Windows development

Use the MSVC Rust toolchain with Visual Studio Build Tools and the Windows SDK. Measure performance with `cargo run --release --locked`. Windows uses Iced's Tiny Skia renderer to avoid costly GPU emulation; clipboard, printing and Office integration are Rust code linked into the app. See the [Windows guide](windows.md).
