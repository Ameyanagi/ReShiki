# Naming dependency notice archive

The naming engines now come from [Ameyanagi/nazuchem](https://github.com/Ameyanagi/nazuchem),
pinned to `f418fea6747aad48d9f5cb020b00b8afc55cbb31` in the workspace dependencies.
Cargo metadata supplies their current complete license texts and notices to
`scripts/license_notices.py`, including files in the git checkout.

These files retain the pre-split notice bytes from ReShiki main at
`1c48d5a02f7e6747da550be3597237263cad4d52`:

- `NOTICE`: the unused facade's scope and dependency provenance notice.
- `nazuchem-kernel-NOTICE` and `nazuchem-s2n-NOTICE`: scope notices whose wording
  changed during repository extraction. Their source paths are historical.
- `Cargo.toml.before-split`: the unused facade's original license and dependency
  declaration, retained as an attribution record after it left the lockfile.

The archive preserves every unique text in the third-party notices generated
before the dependency switch. It supplements the current dependency notices.
