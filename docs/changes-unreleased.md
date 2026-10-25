# Unreleased changes

## Fixes

Crystal files passed to the CLI now produce a clear unsupported-format message
instead of a SMILES parse error. Rejection happens before input is read or output
is created. @Ameyanagi, [PR #537](https://github.com/Ameyanagi/ReShiki/pull/537),
under review. [Command examples and validation](changes/qa-490.md).

See [ReShiki 0.13.0](changes-0.13.md) for the latest release notes.
