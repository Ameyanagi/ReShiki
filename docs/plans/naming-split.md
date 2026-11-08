# Plan: move structure↔name out of ReShiki into an open-source library

Status: accepted 2026-11-09 (decisions D1–D6 made). Name: **nazuchem** (名付け + chem).

## 1. Where things stand (main @ e73e0fdd)

| Part                            | Location                                                                                                                                                                                                                                                      | Size                                                                     | Origin                                                                                                                                                                                                                                          |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| name→structure (n2s)            | `crates/opsin`                                                                                                                                                                                                                                                | ~50k lines Rust (src + tests), 5.7 MB resources, 5.7 MB oracle fixtures  | Port of OPSIN main `42e87ac6` (MIT, Daniel Lowe). Already a standalone crate with `PORTING.md`, `NOTICE`, `provenance/`                                                                                                                         |
| structure→name (s2n)            | `src/naming/openclatura`                                                                                                                                                                                                                                      | 89 files / 54k lines (nested `fusion/`, `phane/`, `rules/`), 960 KB data | Began as a port of openclatura 0.4.3 (MIT, lamalab-org), heavily extended in ReShiki. **Also contains OPSIN-derived data** (`data/fusion_entry_geometry.json`, `retained_macrocycle_templates.json`). Many files have no per-file origin marker |
| App glue                        | `src/naming.rs`, `src/naming/*.rs`                                                                                                                                                                                                                            | ~3.8k lines                                                              | ReShiki original; `catalog` data is OPSIN MIT + ChEBI CC BY 4.0                                                                                                                                                                                 |
| Chemistry the naming code needs | `crates/chemistry`: `smiles`, `kekulize`, `aromaticity`, `rings`, `ranking`, `stereo` (perception, CIP, and the spatial/drawing types they use), `sanitize`, `electronic`, `normalize`, `cx`, `graph`, `hydrogens`, `atomic_data`, `isotopes`, `native_order` | ~25k lines (exact transitive closure to be inventoried in step 1)        | RDKit-derived (BSD-3); extended-tetrahedral CIP from SiMolecule **centres (BSD-2)**                                                                                                                                                             |

Couplings that matter:

- `opsin` → `reshiki-chemistry` only in `modern_cip.rs`. Its result reaches ReShiki as semantic CXSMILES + a parent-only locant projection (`principal_parent_atoms`, first locant, writer atom order, folded H excluded).
- `openclatura` → `reshiki-chemistry`, `crate::naming` (status/reason enums, `IsotopeStyle`, `canonical_smiles`), and through `canonical_smiles` → `molecule_names::canonical_identity` in `crates/model` (admission checks, clearing unknown stereo, radical-electron preservation).
- `aromaticity` (via `sanitize`) calls `Capabilities::detect()` and draws on the process-wide reservation pool in `crates/process-heap`. `fusion` keeps thread-local caches.
- Tests that cross the boundary: catalog tests embed 17 OPSIN XML resources; phane tests use the InChI reader; `s2n_harness` verifies through the worker adapters. `crates/python` `drawing::from_name` is **production** code calling `opsin` directly.
- The ReShiki notice collector (`scripts/license_notices.py`) reads each dependency's **crate directory** only.
- Churn: 89 commits since 2026-09-01 touch naming paths; 29 of 77 open issues are naming bugs.

## 2. Goals and non-goals

Goals

1. ReShiki depends on the naming library like any other dependency; naming bugs, corpora and oracle gates live in the library repo.
2. A typed, versioned structure IR becomes the boundary in both directions (Phase B).
3. General-purpose: Rust, Python (PyPI), JS/WASM (npm), no ReShiki dependency.
4. Provenance explicit per file and per ported function; every distribution channel ships the complete third-party notices.
5. The move itself changes no behaviour: every existing gate is byte-identical before and after.

Non-goals for the move: naming-quality work, algorithm rewrites, switching chemistry kernel. Process supervision, browser allocator ownership, wire/heap limits and the PubChem lookup stay in ReShiki.

## 3. Two phases

**Phase A — move, same interface.** Extract the code into crates with today's interface: SMILES in for s2n, semantic CXSMILES + parent locants out for n2s, worker protocol v4 unchanged. Proven byte-identical, then lifted into the new repo. This alone delivers the development-speed goal.

**Phase B — introduce the IR.** Designed from a complete inventory of what the two engines carry today, added as adapters next to the string interface, gated separately, then made the primary API. Mixing the two phases would bundle five compatibility changes (extraction, IR, parser path, result types, protocol v5) into a move that must stay byte-identical.

## 4. Library layout and crate graph

```
nazuchem/
  crates/
    nazuchem-ir/    original; MIT OR Apache-2.0. Shared vocabulary: NameStatus, NameReason, IsotopeStyle,
                  warnings; (Phase B) the structure IR + JSON Schema + CXSMILES read/write
    nazuchem-kernel/  perception kernel + canonical identity (the pure part of canonical_identity);
                  RDKit BSD-3 + centres BSD-2 + original; published, documented as internal
    nazuchem-n2s/   name → structure; today's crates/opsin renamed; MIT (OPSIN)
    nazuchem-s2n/   structure → name; today's src/naming/openclatura; openclatura MIT + OPSIN MIT data + original
    nazuchem/       facade: Parser, Namer, verify(); features "n2s", "s2n"; re-exports nazuchem-ir
  bindings/python  PyO3 + maturin abi3 → PyPI `nazuchem`
  bindings/wasm    wasm-bindgen → npm `nazuchem` (or `@ameyanagi/nazuchem`)
  corpora/         s2n bake-off/baseline/rules TSVs, openclatura graph oracle
  PROVENANCE.toml  per-file origin ledger (§6)
```

Dependency direction: `ir` ← `chem` ← {`n2s`, `s2n`} ← `nazuchem`. Shared types and identity sit **below** both engines, so the facade never has to be imported by an engine (no facade↔s2n cycle).

Two adjustments needed to keep the library host-neutral without changing behaviour:

- **Budget injection.** `nazuchem-kernel` must not detect machine capabilities or touch a process-wide reservation pool. The aromaticity/sanitize entry points take a `Budget` (default: unbounded); ReShiki passes exactly what `Capabilities::detect().resolve()` and its pool give today, so ReShiki output stays identical.
- **Admission policy split.** `canonical_identity` splits into a pure identity function (to `nazuchem-kernel`) and ReShiki's admission rules (stay in `crates/model`). The s2n engine keeps its own existing rejections (e.g. allene/coordination stereo, N/ring lone-pair centres), so every input rejected today is still rejected.

Not called `opsin` on crates.io: that would read as the upstream project. Docs say "a Rust port of OPSIN by Daniel Lowe".

Library invariants (stated in the README): no I/O, no threads, no process-wide mutable state; bounded thread-local caches exist in fusion. Worst-case time on adversarial input is unbounded; hosts needing hard limits run it supervised, as ReShiki does.

### Chemistry kernel (decision D2)

- **A (recommended): copy the exact transitive subset of `reshiki-chemistry`** → byte-identical, every corpus stays green. Cost: ~25k lines exist twice; fixes are mirrored deliberately.
- B: COSMolKit (`cosmolkit-core/-smiles/-stereo`, 0.5.0-rc) → no duplication but canonical SMILES, Kekulé choice and ranking ties may differ, forcing a re-baseline mid-move. Evaluate later as its own project behind the same corpora.

## 5. The structure IR (Phase B)

nazuchem's own typed graph is the contract; versioned JSON (`{"nazuchem_ir": 1}`, additive-only, unknown variants rejected with a typed error) with a published schema; lossless CXSMILES export (locants as atom values, parent set as atom property); Molfile V3000 export later. Standard formats were compared and rejected as the contract: CXSMILES (uneven extension support, every consumer needs a SMILES parser, canonical reordering loses atom identity), V3000 (verbose, coordinate-based, numbering-dependent parity), CommonChem (order-dependent stereo, no position variation), InChI (identity check only).

```rust
struct Molecule {
    atoms: Vec<Atom>,                            // index = identity; s2n input order preserved
    bonds: Vec<Bond>,                            // Kekulé orders only
    stereo: Vec<Stereo>,
    stereo_groups: Vec<StereoGroup>,             // Abs | And(n) | Or(n) over stereo entries
    position_variation: Vec<PositionVariation>,  // { bond, endpoints: Vec<u32> }
    polymers: Vec<RepeatUnit>,                   // { atoms, crossing_bonds, connectivity: HeadToTail | HeadToHead | Unknown }
    parent: Vec<u32>,                            // principal-parent atoms
}
struct Atom {
    element: u8,                 // 0 = wildcard / attachment point
    isotope: Option<u16>,
    charge: i8,
    hydrogens: u8,               // H not present as vertices; isotopic H always are vertices
    hydrogens_stated: bool,      // [CH4] = true, C = false
    radical_electrons: u8,
    label: Option<String>,       // wildcard attachment / R-group label
    locants: Vec<String>,        // all locants in name order; display = locants[0] if atom ∈ parent
}
enum Stereo {
    Tetrahedral { center: u32, neighbors: [Ref; 4], winding: Winding, known: bool }, // Ref = Atom | ImplicitH | LonePair
    DoubleBond  { bond: u32, refs: [u32; 2], relation: Cis | Trans, known: bool },
    // later, additively: ExtendedTetrahedral, Atropisomer
}
```

`known: false` = ξ / wavy bond; no entry = unspecified. Coordinates are not part of the IR.

The shape above covers these requirements (the engines use all of them today):

1. **Position-variation bonds** (CXSMILES `m:`, e.g. `*CC |$_AP1$,m:0:1.2|`) as typed multi-attachment references.
2. **Complete polymer units** (SRU connectivity `ht`/`hh`/`eu`, brackets) and fully typed `Abs/And/Or` stereo groups, matching what OPSIN serializes today.
3. **Hydrogens as both explicit and total counts** (naming branches on `explicit_h_count`; `C` vs `[CH4]` must stay distinguishable), plus isotopic H kept as graph vertices (never folded into a count) and an input-atom → IR-atom map.
4. **Locants as a defined contract**: per atom, `locants: Vec<String>` (all) plus `parent: bool` and the selected display locant, so ReShiki's current parent-only projection is reproducible exactly.
5. **Aromaticity**: Kekulé orders stay the contract, with a proof obligation: the IR → `nazuchem-kernel` adapter must reproduce `graph_io`'s aromatic-bond snapshot and the fusion audit's localized-pi rejections on the full corpora; if it cannot, carry an `aromatic` bond annotation.
6. **Radicals vs wildcards** remain distinct (genuine radical electrons vs OPSIN wildcard attachment atoms with labels).

Result types must keep today's semantics:

- `GeneratedName`: `text, status, alternatives, omitted_stereo, omitted_units` (unknown units ≠ omitted locants), `atom_locants` with a flag for when they do not apply (radical-ion names suppress the recovered parent's locants), and `verification: { target: Input | RecoveredNeutral { name, smiles }, outcome }`.
- Two parse modes stay separate: strict import parsing and generated-name verification parsing (radicals allowed), as worker v4's `Parse` and `Verify` do today.
- Before switching the worker to IR (protocol v5), measure IR JSON size against the 32 KiB request / 64 KiB response caps and the browser worker's memory limits; raise caps only with evidence.

## 6. Provenance, licensing and IUPAC citations

1. **Rust source files** start with an origin header:
   ```rust
   // SPDX-License-Identifier: MIT
   // Origin: port of OPSIN FusedRingNumberer.java (github.com/dan2097/opsin @ 42e87ac6), © 2017 Daniel Lowe
   // Changes: FR-5.4(b) per-element heteroatom seniority (Nazuchem; diverges from upstream) — see PORTING.md
   ```
   or `// SPDX-License-Identifier: MIT OR Apache-2.0` + `// Origin: original Nazuchem code`. Mixed files mark function-level boundaries: `// Ported from openclatura chains.py::_longest_chain` vs `// Nazuchem extension (not in openclatura 0.4.3): …`.
2. **Data, byte-preserved upstream XML and binary DFA tables** get no inline header (that would break byte equality); they are listed in `PROVENANCE.toml` with origin, upstream path, commit, licence and SHA-256.
3. `PROVENANCE.toml` covers every file recursively (source, data, fixtures, scripts). `cargo xtask provenance` fails CI on a missing entry, a header/ledger mismatch or a hash change, and generates the notices.
4. **Notices travel inside each publishable crate**: every crate directory contains its own `LICENSE*`, `NOTICE` and a `licenses/` folder with the complete upstream texts that apply to it (n2s: OPSIN; s2n: openclatura + OPSIN; chem: RDKit + centres; facade: all). Wheels and npm packages include the same. Release CI asserts the **contents**, not just filenames, of the `.crate`, wheel and npm tarball, and ReShiki's CI asserts its aggregated `THIRD-PARTY-NOTICES.txt` still contains OPSIN, openclatura, RDKit and centres after the switch. During Phase A, the airlock crates are not `MIT OR Apache-2.0` as a whole, so the collector's "original workspace crate" shortcut must not skip them.
5. **Wording**: README, crate docs and package metadata say "port of OPSIN" / "began as a port of openclatura 0.4.3, extended" — never "original" for those parts.
6. **IUPAC references**: rule functions carry a doc tag such as `/// IUPAC 2013: P-44.1.2.2`; fused `FR-`, spiro `SP-`, von Baeyer `VB-`, ring assemblies `RB-`. Rule numbers and short quotes only (Blue Book text is IUPAC/RSC copyright). A script turns the tags into a rule-coverage page. Note that OPSIN-compatibility data (e.g. `fusion_entry_geometry.json`) already says it is "parser compatibility, not IUPAC PIN certification"; keep that distinction.
7. **Licences**: original code MIT OR Apache-2.0; OPSIN and openclatura MIT; RDKit BSD-3; centres BSD-2; ChEBI CC BY 4.0 (only if the catalog moves).

**Provenance audit** (step 2): all 89 openclatura files and its data against openclatura 0.4.3 (`90dc5bcb`) plus ReShiki git history, separating openclatura-ported, OPSIN-derived and original code, function by function; the chemistry subset against RDKit/centres. The output _is_ the first `PROVENANCE.toml`.

## 7. Migration steps

Baseline first: freeze the outputs of the OPSIN gates (8,100 lexical, 2,624 structure, 640 configuration), all s2n corpora, the openclatura graph oracle, and ReShiki's naming/app/Python tests. Every Phase A step must reproduce them byte-for-byte on macOS, Arch Linux x64, Ubuntu arm64 and Windows x64 (local test hosts first, CI only as the final check).

**Phase A (one ReShiki PR per step, each independently reviewed)**

| Step | Work                                                                                                                                                                                                                            |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0    | Land or close in-flight naming PRs                                                                                                                                                                                              |
| 1    | Inventory the chemistry transitive closure; create `crates/nazuchem-kernel` (exact copy + `Budget` injection); point `opsin` and openclatura at it                                                                              |
| 2    | Provenance audit → `PROVENANCE.toml`, headers, per-crate `licenses/`, collector fix                                                                                                                                             |
| 3    | `crates/nazuchem-ir` with the shared vocabulary only; split `canonical_identity`                                                                                                                                                |
| 4    | Move openclatura → `crates/nazuchem-s2n` (SMILES in, unchanged `Name` out); move its tests and fixtures, replacing the worker-adapter harness and InChI-reader phane tests with library-local support                           |
| 5    | Rename `crates/opsin` → `crates/nazuchem-n2s`; facade crate `nazuchem` with today's string API; switch ReShiki callers incl. `crates/python` `from_name`; keep catalog XML evidence pinned                                      |
| 6    | `git filter-repo` the crates, corpora and licences into the new repo (history kept); ReShiki depends by git rev and deletes its copies. Real-app check: name import + locant hover, generated name panel, browser naming worker |
| 7    | New repo CI: fmt, clippy, tests, oracle gates, provenance, 3 OS + wasm32. Transfer naming issues (`gh issue transfer`)                                                                                                          |

**Phase B (in the new repo)**

| Step | Work                                                                                                              |
| ---- | ----------------------------------------------------------------------------------------------------------------- |
| 8    | IR v1 to §5's requirements; adapters both ways next to the string API; round-trip and corpus gates through the IR |
| 9    | Python and npm bindings on the IR API                                                                             |
| 10   | ReShiki: `IR → chemistry State` converter, worker protocol v5 after the size measurement                          |
| 11   | 0.1.0 via tag-triggered trusted publishing (crates.io, PyPI, npm); ReShiki moves from git rev to version          |

Risks: step 4 is the hardest (openclatura uses `crate::naming` in 114 places; most are the enums). The two chemistry copies can drift; mirror fixes deliberately. The npm package carries ~5.7 MB of OPSIN resources; features allow n2s-only or s2n-only builds.

## 8. Name

**nazuchem** (decided 2026-11-09). Free on crates.io, PyPI, npm (`nazuchem`, `@ameyanagi/nazuchem`) and `github.com/Ameyanagi/nazuchem` when checked. Crates: `nazuchem` (facade), `nazuchem-ir`, `nazuchem-kernel` (chemistry; avoids `nazuchem-chem`), `nazuchem-n2s`, `nazuchem-s2n`.

## 9. Decisions (made 2026-11-09)

- D1 Name: nazuchem.
- D2 Chemistry kernel: copy the reshiki-chemistry subset now; evaluate COSMolKit later as its own project.
- D3 Repository: github.com/Ameyanagi/nazuchem.
- D4 Licence for original code: MIT OR Apache-2.0.
- D5 Naming work during Phase A: freeze naming features and non-urgent fixes until step 6.
- D6 IR: nazuchem's own typed graph with versioned JSON (§5).
- Optional courtesy: tell OPSIN's author and lamalab-org before the first public release.
