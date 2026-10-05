# Python/RDKit reference

This directory holds everything that needs, or captures from, the pinned Python/RDKit reference. It is used only with the `rdkit-reference` Cargo feature; normal builds and release packages never include it.

- `engine/` is the Python worker behind `PythonEngine`.
- `<name>.rs` files are `rdkit-reference` integration targets of the `reshiki` package, each declared in `Cargo.toml` with `path = "reference/<name>.rs"`. Their helpers live in `support/`, `common/` and `drawing_exchange/`.
- `*_reference.py`, `build_*_oracle.py`, `*.cpp` and `*.h` files are oracles, builders and native observers.
- `test_*.py` files are the Python reference tests.
- The Markdown notes record how fixtures were captured.

Fixtures stay in `../tests/fixtures/`; the drawing style shared with the application is `../assets/drawing_style.json`.

Run `uv sync --locked --python 3.12` first: the tests use the checkout's `.venv`, or the interpreter named by `RESHIKI_REFERENCE_PYTHON`. See [development](../docs/development.md) for platform setup.

```sh
cargo test --workspace --locked --features rdkit-reference --no-fail-fast
cargo test --locked --features rdkit-reference --test <name>
uv run --locked python -m unittest discover -s reference -p 'test_*.py'
```

Add new `rdkit-reference` targets here as `reference/<name>.rs` with an explicit `[[test]]` path; `test_reference_features.py` enforces this.
