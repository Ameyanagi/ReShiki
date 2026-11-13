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

## Pinned RDKit

The reference is RDKit 2026.9.1 (`rdkit==2026.9.1`, source `Release_2026_09_1`, commit `fece8caa860bdf6c9c82bdb9253f22a0cf87150c`). RDKit 2026.09.2 is tagged but its wheel is not on PyPI yet; the pin moves to 2026.9.2 when it ships.

Every reference run uses RDKit's legacy ring symmetrization, which ReShiki keeps: each live oracle and capture script calls `Chem.SetUseLegacyRingFinding(True)` before any RDKit work, and native observers they start inherit it. New oracles must do the same.

## Deliberate differences

ReShiki keeps these behaviours where RDKit 2026.09.1 differs. The affected oracles count the cases by rule and print the count; any other mismatch still fails.

- Ring finding ([rdkit#9105](https://github.com/rdkit/rdkit/pull/9105)): RDKit takes its SSSR basis from RingDecomposerLib, even with legacy symmetrization. ReShiki keeps its Figueras basis and symmetrization. `ring_perception` expects a mismatch only when ReShiki's basis rings differ from RDKit's. Oracles that compare whole states, including `reaction_drawing`'s participants, take RDKit's ring list only under the same rule and still compare every other field (`support/ring_basis.rs`). `engine_migration` and `native_import` accept an import response that differs only when RDKit's basis for the drawing differs from ReShiki's (`support/drawing_basis.rs`), since the layout is built from those rings, and its dense-ring fixture keeps 33 rings where RDKit finds 34.
- Import template retry ([#538](https://github.com/Ameyanagi/ReShiki/issues/538), [#1114](https://github.com/Ameyanagi/ReShiki/issues/1114)): ReShiki's import retries a bridged molecule with its ring template when two atoms are closer than half a bond, also in RDKit-parity runs. RDKit 2026.09.1's new placement ([rdkit#9650](https://github.com/rdkit/rdkit/pull/9650), [rdkit#9378](https://github.com/rdkit/rdkit/pull/9378)) does that to the agent of `molecule/NCI/3501/1`, which the frozen worker keeps. `engine_migration` accepts a response only when RDKit's drawing has such a bridged participant, ReShiki drew that participant differently, and the only differences are its positions and the atom x positions and arrow ends derived from its width (`support/import_template_retry.rs`); it lists those cases.
- Ring type label ([rdkit#9753](https://github.com/rdkit/rdkit/pull/9753), fixed in 2026.09.2): after legacy symmetrization RDKit 2026.09.1 reports the rings as an SSSR basis. The InChI import captures keep RDKit's label; `inchi_reader` counts "basis" against ReShiki's "symmetric" for the same rings.
- CX numbers ([rdkit#9273](https://github.com/rdkit/rdkit/pull/9273)): RDKit reads them with C `atoi`/`atof`, so it silently accepts malformed, padded, hexadecimal, overflowing and underflowing numbers (`C |(bad,2,)|` becomes `(0,2,)`). ReShiki keeps the strict reader of 2026.03.6. `smarts`, `smiles_read`, `reaction_smiles` and the SMILES engine import oracles expect a rejection only when RDKit accepts and the CX extension holds such a number (`support/cx_lenience.rs`); `cx_coordinate` expects it for every value only RDKit reads. Through a molfile SMARTSQ, RDKit now applies a query it used to ignore, so the molfile, RXN and engine import oracles expect ReShiki to accept where RDKit refuses only for such SMARTSQ data. The same `atoi` reads stereo-group ids: the MSVC runtime saturates an id beyond INT_MAX at 2147483647 on Windows x64, where glibc and macOS keep it modulo 2^32 as ReShiki does. The oracles renumber colliding ids before RDKit reads them (`support/cx_stereo_groups.py`); `reaction_smiles` compares a remaining saturated id as RDKit's on Windows x64 only and counts it.
- CDXML nicknames ([rdkit#9567](https://github.com/rdkit/rdkit/pull/9567)): RDKit reads an unknown generic nickname such as `Ar` or `XH` as carbon; ReShiki keeps a dummy atom. RDKit reads an exact `A`, `a` or `Q` as an atom query; ReShiki has no query atoms, so its molecular CDXML parser refuses them and the application imports them as labelled dummy atoms with a warning. `cdxml_molecular` and `cdxml_preparation` count both (`support/query_nicknames.rs`).
- Atropisomers from wedges ([rdkit#9190](https://github.com/rdkit/rdkit/pull/9190)): RDKit now perceives a wedged biaryl axis in reaction SMILES, which the original worker refuses as unsupported bond stereo. `native_response` and `engine_migration` expect that refusal; the molfile and RXN import oracles check ReShiki's axis with `support/axial_extension.rs`. The worker also reads a `bold` display as a wedge, so RDKit can perceive an axis on a bold ester bond; `abbreviation_replacement` counts that refusal when the only wedges are bold bonds. Bold is not stereo input in ReShiki. On metal chelates such as `CN(C)C1=[S+][Zn]2(S1)SC(=[S+]2)N(C)C`, RDKit's new detection fails its own precondition (`bad bond size`); `drawn_stereo` counts that failure.
- Atropisomer axes in RDKit-parity runs ([#1117](https://github.com/Ameyanagi/ReShiki/issues/1117)): `engine_migration` imports in RDKit-parity mode (#1037), which lays a molecule out again and wedges it by RDKit's plain rules. For RDKit's `AtropManyChirals.cxsmi` and its three expected `.cxsmi` outputs, that drawing loses the axis and its M label, depending on the platform's layout. RDKit 2026.09.1's own relayout is as layout-sensitive: it loses the axis of `.expected3`. `engine_migration` counts these four cases by name only when parity mode loses the axis (`support/parity_atropisomers.rs`, which holds the inputs). `atropisomer_normal_mode` imports them without parity mode, as users do, on every platform, and requires RDKit's axis with the same winding and the M label; with `RESHIKI_RDKIT_SOURCE` set, it also checks that the inputs still match the source.
- CIP selections ([rdkit#9030](https://github.com/rdkit/rdkit/pull/9030)): with the nanobind wrapper, an empty atom or bond selection labels nothing. ReShiki keeps Boost.Python's meaning, which labels everything. The 2026.9.1 PyPI wheel still uses Boost.Python, so no oracle sees the difference.

[rdkit#9484](https://github.com/rdkit/rdkit/pull/9484) (wedges on bridges) is in 2026.09.2, not 2026.09.1. It is not ported until it is checked against a 2026.09.2 build.

Add new `rdkit-reference` targets here as `reference/<name>.rs` with an explicit `[[test]]` path; `test_reference_features.py` enforces this.
