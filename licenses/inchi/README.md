# InChI attribution

Runtime generation and import use `cosmolkit-inchi = 0.3.0`, a pure Rust source
port of InChI 1.07.5 and the RDKit adapter, from the
[Ameyanagi/COSMolKit fork](https://github.com/Ameyanagi/COSMolKit).
Cargo.toml and Cargo.lock pin commit `3a437849dcd28319b1a3cdf02d7897a7ee200cb2`
and the transitive dependency graph. The fork's `FORK.md` documents the hydrogen,
perchlorate, unknown-stereo and string-buffer compatibility corrections. The COSMolKit MIT
license is retained under `licenses/rust/` at the original source revision;
the fork retains the RDKit BSD notice alongside its source. Release notice
collection includes both. No InChI C source is built or linked by the application.

The remaining notes describe ReShiki's independently adapted InChIKey code and
the older C implementation used only to capture development reference data.

The Rust InChIKey implementation in `src/chemistry/inchi/key.rs` adapts the
key parsing, layer separation and base-26 encoding from the official IUPAC
InChI reference implementation, version 1.07.3. These sources are MIT licensed,
copyright 2024 IUPAC and InChI Trust. The project-level license additionally
names the InChI Project; both notices are retained in LICENSE.

Upstream: <https://github.com/IUPAC-InChI/InChI>

Tag: `v1.07.3`, commit `0b3e941d29289f3e5024c9ecfd45186285319420`.
The source archive is the same one pinned by RDKit 2026.03.6, commit
`0e0d85f4ca34aeae15dfc0f7cf5503bdb0a8e985`, in
`Code/cmake/Modules/FindInchi.cmake`:

<https://github.com/IUPAC-InChI/InChI/releases/download/v1.07.3/INCHI-1-SRC.zip>

Archive SHA-256:
`b42d828b5d645bd60bc43df7e0516215808d92e5a46c28e12b1f4f75dfaae333`.

Adapted source files under `INCHI-1-SRC/INCHI_BASE/src`:

- `ikey_dll.c`: `GetINCHIKeyFromINCHI`, SHA-256
  `b68b0cc1a2e0d1df9a56a7e1232658b3faf4db026354237a9a5a8d514ac7c1d5`.
- `ikey_base26.c`: the native triplet table and digest bit selection, SHA-256
  `0e3893bfc3f218aa5574fa87ddbf7cbc5d2c997e398c57073c43b87ed867c056`.
- `util.c`: `extract_inchi_substring`, SHA-256
  `0a785f94a5df5775fcecabf0844068ae744d1630b0276c14e86a39b17ef37027`.

SHA-256 hashing uses RustCrypto's `sha2` crate with its software backend. The standalone key implementation has no native dependencies.

Initial character validation uses ASCII/C-locale rules. The native Windows
`isalnum` check can accept a UTF-8 leading byte in some locales, then discard
the non-ASCII body and hash an empty layer. Rust rejects those malformed inputs;
generated chemical identifiers are ASCII and retain their exact keys. Tests
compare both the C locale and the original process locale, reporting this
malformed-input restriction separately.

The runtime reference test calls the installed pinned native
`GetINCHIKeyFromINCHI` directly and checks the public RDKit wrapper as well. Its
optional 1,181-entry PubChem hard set is read from the pinned RDKit checkout or
`RESHIKI_INCHI_HARD_SET`; the dataset is not redistributed here. The source file
`rdkit/Chem/test_data/pubchem-hard-set.inchi` has SHA-256
`603b0a620db6aa664c399f616b162a75ecd91cc56994099aab0c8bbaee2d4a28`
after CRLF-to-LF normalization. Set `RESHIKI_REQUIRE_INCHI_HARD_SET=1` to require
that corpus during validation.
