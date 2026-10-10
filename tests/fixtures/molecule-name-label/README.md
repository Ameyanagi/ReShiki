# Compact Import Name and linked-caption evidence

These are new follow-up assets. Existing `chemical-naming-rust` captures,
fixtures, source manifests, licenses and raw receipts are unchanged. Desktop
captures were made by the root native verification session, using the exact
signed bundle recorded in [the provenance receipt](validation/bundle-provenance.json.raw).
The [native acceptance receipt](native-acceptance-v1.json.raw) reports all seven
checks passing. Its original SHA256 is
`1333368130459d3ff76cef6ae09fe91f92a57fdb7158ef46cc0e947b3de926a4`.

| Native file | Verified saved contents |
| --- | --- |
| [ethanol-direct-import.rsk](ethanol-direct-import.rsk) | 3 atoms, 2 bonds, one `ethanol` caption and link |
| [ethanol-generated-caption.rsk](ethanol-generated-caption.rsk) | Same exact atoms/bonds, one `ethan-1-ol` caption and link |
| [ethanol-structure-only.rsk](ethanol-structure-only.rsk) | 3 atoms, 2 bonds, no annotation or name link |

The image files are under `docs/images/molecule-name-label`. The `before-*`
images are actual earlier desktop captures, not generated concepts. All other
images and adjacent `*-ax.txt` snapshots correspond to the new native review.
The [artifact index](artifact-index.json) maps repository files to their
original paths, byte counts and SHA256 values. Files ending in `.json.raw`
preserve the original JSON bytes and hashes; they are not reformatted copies.

The [final focused validation receipt](validation/validation-final.json.raw)
records 57 distinct passing tests, strict Clippy, formatting, exact-source
build and package checks. The [frozen source manifest](validation/frozen-source-v2.json.raw)
contains the tested source hashes. `build2.log` is the complete successful Cargo
JSON stream. `default-app-v2.d` is the original source-qualified depfile. The raw
executable SHA256 is
`c2c7ea9272c90e9020e93b00927878a776a6c653571193ee68b59e29d3921bb2`;
the signed executable SHA256 is
`ed4e1cfe393e9e192f300928a22556fe215cb25e86dbefcca9f42f06e1af9a0f`.
Receipts describe a locked default-feature debug build for macOS arm64.
Subsequent documentation/asset changes do not change executable inputs, as
verified in the [runtime source bridge](runtime-source-bridge.json). The
[full normal commit-hook log](validation/full-commit-hooks.log) records
all-feature Cargo check and strict Clippy, Cargo fmt, oxfmt and oxlint passing.
The [root independent verification](validation/root-focused-verification-v1.json.raw)
checks the named focused-test passes and original log hashes.

Every failed attempt is preserved beside its successful rerun:

- `build1`: ambiguous `column!` import, corrected before the final source freeze.
- `model1`: inherited nonexistent stereo-test field, corrected to `winding`.
- `package1`: system Python 3.9 lacked `tomllib`; Python 3.14 succeeded.
- `app-naming1`: tests relaunched their test executable for InChI. The rerun set
  both worker overrides to the immutable exact-source application and passed.

Reproduce the focused checks from this source on macOS. Build the ordinary
application first; test executables require the explicit local worker paths.

```sh
export CARGO_BUILD_JOBS=4
cargo build --locked -p reshiki --bin reshiki
export RESHIKI_NAMING_HELPER="$PWD/target/debug/reshiki"
export RESHIKI_INCHI_HELPER="$RESHIKI_NAMING_HELPER"
export RESHIKI_REQUIRE_INCHI_HELPER=1
cargo test --locked -p reshiki-model molecule_names:: -- --test-threads=1
cargo test --locked -p reshiki --bin reshiki app::naming:: -- --test-threads=1
cargo test --locked -p reshiki --bin reshiki app::context_menu:: -- --test-threads=1
cargo test --locked -p reshiki --bin reshiki app::import:: -- --test-threads=1
cargo test --locked -p reshiki --bin reshiki app::naming::import_dock::tests::default_dock_exposes_one_insert_and_collapses_preview_and_details -- --ignored --exact --test-threads=1
cargo test --locked -p reshiki --bin reshiki canvas::tests::secondary_click_selects_its_target_and_preserves_an_existing_multi_selection -- --exact --test-threads=1
cargo clippy --locked -p reshiki -p reshiki-model --all-targets -- -D warnings
cargo fmt --all -- --check
```

For native acceptance, open Import → Name, type `ethanol`, keep Add name below
selected and press Insert. Preview and Details remain closed. One Undo removes
the graph and caption, and one Redo restores both. Right-click the molecule to
Hide or Show its verified name. Repeat in a new tab with Add name below off,
then reopen the direct-import fixture into a fresh tab. The saved caption
returns, while Undo and Redo start disabled.
