# ReShiki for coding agents and contributors

ReShiki is a Rust 2024 workspace (an Iced desktop app plus headless crates)
with an Astro documentation site, a browser editor and a Python package.
Read these before larger work:

- [docs/architecture.md](docs/architecture.md): crates, layering, worker modes.
- [docs/development.md](docs/development.md): setup, test organisation,
  opt-in renderer tests, golden baselines and how to regenerate them.
- [docs/writing-style.md](docs/writing-style.md): wording for every
  user-visible text, guide page and release note.
- [docs/README.md](docs/README.md): where each kind of document goes.

## Checks by changed path

Run the checks for every path you change, and fix them before you push. CI is
only a check: it should confirm what already passed on your machine.

| Changed path                                        | Run                                                                                                      |
| --------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `crates/<name>/`                                    | `cargo test -p reshiki-<name> --locked` (`crates/web-worker` is `reshiki-web-worker`)                    |
| `native/<name>/`                                    | `cargo test -p reshiki-<name> --locked` (`reshiki-macos`, `-windows`, `-linux`, `-geometry`, `-process`) |
| `src/<module>` declared in `src/main.rs`            | `cargo test --locked --bin reshiki <path>::`, e.g. `app::<module>::` or `session::`                      |
| `src/<module>` declared in `src/lib.rs`             | `cargo test --locked -p reshiki --lib <module>::`                                                        |
| `tests/<name>.rs`                                   | `cargo test --locked -p reshiki --test <name>`                                                           |
| `vendor/`                                           | `uv run --locked python scripts/check_vendor.py`, then the tests of the crates that use it               |
| `scripts/*.py`, `tests/test_*.py`, `reference/*.py` | `uv run --locked python -m unittest tests.test_<name>`; `ruff check`, `ruff format --check`, `ty check`  |
| `crates/python/`                                    | the commands in [docs/python.md](docs/python.md)                                                         |
| `packages/editor/`, browser code                    | `scripts/build_wasm.sh --test` and `bun run lint:js`                                                     |
| `docs/`, `website/`                                 | `bun run docs:check` and `bun fmt --check <files>`                                                       |
| `.github/workflows/`                                | `uv run --locked python -m unittest discover -s tests -p 'test_*.py'`                                    |

Then run each touched crate's whole suite, and for any Rust change:

```sh
cargo fmt --all --check
cargo clippy -p <crate> --all-targets --locked -- -D warnings
cargo clippy --workspace --lib --bins --all-features --locked -- -D warnings  # the pre-commit hook
```

CI runs the workspace with cargo-nextest and `--no-default-features`;
[docs/development.md#ci](docs/development.md#ci) has the exact commands and
`.config/nextest.toml` the serial test groups. Lefthook (`lefthook.yml`) runs
fmt, Clippy, Oxlint, `bun fmt`, Ruff and ty on staged files at commit time.

### Environment for tests

- **Worker helpers.** Naming and InChI tests start the built app in a worker
  mode. Build it, then point the tests at it with absolute paths:

  ```sh
  cargo build --locked --bin reshiki
  export RESHIKI_INCHI_HELPER="$CARGO_TARGET_DIR/debug/reshiki"   # target/debug/reshiki by default
  export RESHIKI_NAMING_HELPER="$RESHIKI_INCHI_HELPER" RESHIKI_REQUIRE_INCHI_HELPER=1
  ```

  Rebuild after changing naming or worker code: the tests run the binary as
  built.

- **Data folder.** Every GUI run and every app-level renderer test gets a
  fresh `RESHIKI_DATA_DIR=$(mktemp -d)`, so your real recovery drafts and
  templates stay untouched and cannot change a result.
- **Target folder.** Give each worktree its own `CARGO_TARGET_DIR`. A shared
  one reuses crates built from another worktree's sources, so a test can pass
  or fail on code that is not yours.
- **Failures.** A failing test is a finding. Run it on `main` too and report
  both results.

## Code invariants

- `unsafe` is forbidden (`#![forbid(unsafe_code)]`) in the app and every
  crate except the FFI and allocator crates: `native/macos`, `native/windows`,
  `native/process` and `crates/process-heap`. Keep new `unsafe` inside those.
- Outside tests, the lint block at the top of `src/lib.rs`, `src/main.rs` and
  most crates' `lib.rs` denies `unwrap`, `expect`, `panic!`, `unreachable!`,
  `todo!`, `unimplemented!` and slice indexing. Return errors and use `get`.
- Errors are typed: `thiserror` enums or a plain message, in libraries and the
  app alike. `anyhow` appears only in tests, examples, `build.rs` and
  `native/windows`.
- Panics unwind: the geometry worker recovers with `catch_unwind`, and tokio
  turns a panicked task into an error. Keep `panic = "abort"` out of the
  native profiles (only the WebAssembly build sets it).
- The release ships one executable. InChI, geometry, naming and platform
  services are modes of it, not extra binaries.

## Release notes

Each pull request with a change worth noting adds one fragment,
`docs/changes/unreleased/<branch-with-slashes-as-dashes>.md`. The format is in
[docs/changes/unreleased/README.md](docs/changes/unreleased/README.md). Leave
`docs/changes-unreleased.md` and `docs/contributors.md` to release
preparation; the fragments are what keeps parallel pull requests free of
conflicts.

## Pull requests

- Target `main`. Write the description in the style of
  [docs/writing-style.md](docs/writing-style.md).
- UI changes show before and after screenshots of the same input; issues show
  screenshots of the current behaviour. Store the images in the
  `ReShiki-evidence` repository and embed them in the pull request.
- Tick both boxes under **Contribution license** in the template.
- Name test machines by platform only: "macOS arm64", "Arch Linux x64",
  "Ubuntu arm64", "Windows x64". Keep host names and addresses out of public
  text.
- Verify on macOS, Linux and Windows before pushing. Ordinary pull requests
  run CI on Linux and Windows; macOS runs on stacked `merge/*` pull requests,
  pull requests labelled `full-ci`, `main`, nightly and release builds.
