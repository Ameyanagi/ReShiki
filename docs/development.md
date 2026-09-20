# Development

ReShiki uses Rust for the desktop application, Python/RDKit for chemistry, and Astro Starlight for this documentation site.

## Set up

Install Rust 1.95 with rustfmt and Clippy, uv, and Node.js 24 LTS.

```sh
uv sync --locked --python 3.12
npm ci
cargo run --locked
```

`npm ci` installs Lefthook's Git hooks automatically. `npx --no-install lefthook install` reinstalls them if necessary. Commands run from the repository root.

## Pre-commit checks

Lefthook checks staged changes by file type. It runs Oxlint and Oxfmt for web/configuration/documentation files; Ruff lint/format and ty for Python changes; and Cargo fmt, Clippy, and check for Rust changes. Dependency/configuration changes also trigger the relevant checks. Commands check formatting without rewriting or restaging your work. Rust commands run sequentially to avoid competing for Cargo's build lock.

Run the same checks directly:

```sh
npm run lint:js
npm run format:check
uv run --locked ruff check engine scripts tests
uv run --locked ruff format --check engine scripts tests
uv run --locked ty check
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo check --all-targets --locked
```

To format files, use `npm run format`, `uv run --locked ruff format engine scripts tests`, and `cargo fmt --all`. The first adoption reformatted the existing Python sources and tests so new work starts from a passing baseline. ty checks production Python and build scripts; tests also receive Ruff checks.

## Tests

```sh
cargo test --locked
uv run --locked python -m unittest discover -s tests -p 'test_*.py'
cargo run --locked -- --engine-check
```

Native macOS tests skip on other systems. CI runs Rust checks/tests on macOS, Python checks/tests on all five release targets, and web checks on Linux. Release packaging separately verifies first-use uv setup and offline reuse on all five native targets.

## Documentation

```sh
npm run docs:dev
npm run docs:check
npm run docs:build
```

Edit guide text in `docs/*.md`. A sync script creates Starlight pages, preserving links and pointing each Edit page action to the original Markdown. Generated copies are ignored by Git. Edit the landing page, navigation and theme in `website/`. The production build also checks local page and asset links.

The Documentation workflow builds pull requests and deploys main to [GitHub Pages](https://reshiki.com/). Pages must use **GitHub Actions** as its source. Search, light/dark themes and responsive navigation are supplied by Starlight.

See [release builds and signing](releasing.md) for distribution setup.
