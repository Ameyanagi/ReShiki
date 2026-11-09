# OPSIN naming integration fixtures

The 17 XML files in `xml/` provide the exact token evidence cited by ReShiki's
retained-name catalog. `phanes.txt` supplies upstream name/InChI pairs for
ReShiki's application naming integration tests. These files are test data;
the command palette's runtime element-name resource is kept separately in
[`resources/naming/opsin`](../../../../resources/naming/opsin/README.md).

The files are unchanged copies from [nazuchem revision
`f418fea6747aad48d9f5cb020b00b8afc55cbb31`](https://github.com/Ameyanagi/nazuchem/tree/f418fea6747aad48d9f5cb020b00b8afc55cbb31),
originally from [OPSIN `3.0-SNAPSHOT`, commit
`42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91`](https://github.com/dan2097/opsin/tree/42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91),
copyright 2017 Daniel Lowe, under the [MIT license](../../../../resources/naming/opsin/LICENSE).
The shared [provenance manifest](../../../../resources/naming/opsin/provenance.json)
records source paths, byte lengths and SHA256 hashes. The catalog's existing
OPSIN revision, resource fingerprint and individual XML hashes remain pinned
in [`src/naming/catalog/provenance.json`](../../../../src/naming/catalog/provenance.json).
