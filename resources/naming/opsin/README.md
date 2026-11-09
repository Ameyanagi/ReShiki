# Retained OPSIN data

`elementaryAtoms.xml` supplies element names for ReShiki's command palette.
The [catalog XML and phane fixtures](../../../tests/fixtures/naming/opsin/README.md)
support ReShiki's naming integration tests. These selected files are unchanged
data copies from [nazuchem revision
`f418fea6747aad48d9f5cb020b00b8afc55cbb31`](https://github.com/Ameyanagi/nazuchem/tree/f418fea6747aad48d9f5cb020b00b8afc55cbb31),
which is the external naming dependency pinned by the workspace. No naming
engine source or generated parser tables are retained here.

The original source is [OPSIN `3.0-SNAPSHOT`, commit
`42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91`](https://github.com/dan2097/opsin/tree/42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91),
copyright 2017 Daniel Lowe, under the MIT license. [LICENSE](LICENSE) is the
unchanged upstream license. XML files originate in
`opsin-core/src/main/resources/uk/ac/cam/ch/wwmm/opsin/`; `phanes.txt` originates
in `opsin-inchi/src/test/resources/uk/ac/cam/ch/wwmm/opsin/`.

[provenance.json](provenance.json) records every retained file's location,
purpose, original nazuchem path, byte length and SHA256. Each copy was checked
against both the former local crate and the pinned external dependency.
