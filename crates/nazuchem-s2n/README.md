# nazuchem-s2n

Structure-to-name engine that began as a port of openclatura 0.4.3, extended
in ReShiki. This crate provides its SMILES input, `Name` output, naming rules
and regression data as a reusable library.
It uses `nazuchem-ir` for shared result vocabulary and `nazuchem-kernel` for
molecular perception and canonical identity. Entry points that perform
perception accept an explicit `Budget`; `Budget::default()` is unbounded.

The library performs no I/O, creates no threads and has no process-wide
mutable state. Fusion retains bounded thread-local caches. Worst-case time
on adversarial input is unbounded; hosts needing hard limits should supervise
requests externally. Tests use a development dependency on the Rust port of
OPSIN; app document and InChI integration checks stay in ReShiki.

The source and registries incorporate openclatura MIT material, OPSIN MIT
data and a CPython integer-set adaptation under Python-2.0. Original
ReShiki/nazuchem contributions use MIT OR Apache-2.0. Complete texts and
notices are included in `licenses/openclatura/`, `licenses/opsin/` and
`licenses/cpython/`; `LICENSE-MIT` and `LICENSE-APACHE` cover local
contributions. The manifest combines the established terms as
`MIT AND (MIT OR Apache-2.0) AND Python-2.0`.

That expression does not resolve the reuse terms of
`src/data/bluebook/p25_retained.tsv`. Its ledger licence remains
`NOASSERTION`, pending the owner's IUPAC table reuse-permission decision;
`publish = false` remains in force. Some inherited OPSIN generation revisions
and the CPython patch release remain unrecorded, as qualified in `NOTICE`
and the repository-root `PROVENANCE-nazuchem.toml`. The ledger preserves
historical source paths after the move. ReShiki keeps the root notices while
it retains the corresponding implementations and reference corpora.
