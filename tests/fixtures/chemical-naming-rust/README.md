# Pure Rust naming evidence

These files belong to the clean editor integration stacked on parser
[PR #291](https://github.com/Ameyanagi/ReShiki/pull/291), parent
`0e6e9009ca2e8bfebe149c260fc5a2f484f7dfb9`. They do not reuse the rejected
Java/API prototype's screenshots or compiler evidence.

The 14 original JPEG screenshots are in
[`docs/images/chemical-naming-rust`](../../../docs/images/chemical-naming-rust).
Their 14 accessibility snapshots, three native saves and original root
`native-verification.json` are here. Screenshots are unmodified 2560 × 1704
captures. The current-app `ethanol-before-local-parse` image is a workflow
before-state, not a preimplementation build. The separate
`preimplementation-import-ui-unique` capture comes from the Projection build at
`427d5f4e`, whose five complete Import/app UI source paths match parent
`0e6e9009` byte for byte. That build contains separate projected-double-bond
changes. Its original compilation log and compact 1,063-input source-parity
receipt are included here; an exact parent binary is not claimed. The final
baseline uses a unique bundle ID. Its byte-identical copy was renamed and
re-signed without source or compilation changes; a separate package bridge
binds the original and new signed executable. The earlier shared-ID capture
is preserved privately as supplemental and excluded from this package.

From the repository root, using Python 3.9 or newer with no extra packages:

```sh
python3 tests/fixtures/chemical-naming-rust/verify_native.py
python3 tests/fixtures/chemical-naming-rust/verify_provenance.py
```

The first command checks recorded artifact hashes, ethanol C-C-O/C2H6O,
R-lactic acid C3H6O3 against the explicit reference `C[C@@H](O)C(=O)O`, and
the caption-only native delta. Its canaries ignore cached CIP/hydrogen labels
and reject inverted winding or changed charge/isotope. This is a finite
fixture checker, not a general CIP or chemical naming implementation.
Independent development-only RDKit reconstruction in
`independent-current-native-semantics.json` additionally checks native winding
and a separate wedge/coordinate route; RDKit is not shipped or required by
these commands.

The second command checks the 1,226 recorded source/provenance inputs,
including 1,068 runtime inputs, all 34 integration paths, and compiler,
package and desktop receipt bindings. It reads the original absolute-path
compiler evidence as historical data and compares the corresponding sources
in the current checkout. It does not need the original app or build directory
and does not rebuild or repeat GUI actions.

The original source manifest's complete-patch field is `null`.
`final2-source-patch-correlation-bridge.json` records that omission and binds
the complete patch `f237bcec…`; the original manifest was not rewritten.
Likewise, compilation/package receipts still say GUI pending because they
precede the final root desktop receipt. `native-verification.json` records
the subsequent actual desktop acceptance. Similarly, the baseline source
receipt precedes `preimplementation-unique-native-verification.json`; none of the
older receipts are relabeled.

Two original timing-related fixture failures remain in
`lib-naming-tests.log` and `lib-naming-final-helper-tests.log`. They concern an
aggregate escaped-pipe elapsed assertion and cancellation-fixture startup,
respectively. Their original timing causes were not established. The final
test-only readiness gates preserve actual completion, reaping, private-folder
and production resource limits; focused phase logs and the final 22-test
helper run are retained alongside them.

Native GUI history/reopen was observed by root. Only three native originals
were saved; this package does not invent separate history/reopen byte copies.
The separate signed-worker sandbox receipt records five public examples with
networking denied and Java/Python/helper environment variables removed. The
desktop itself was not run under that network-denial wrapper. GUI Cancel was
not exercised; compiled cancellation and process-lifecycle tests are reported
separately. Six-target portability is compile evidence, not native runtime
acceptance on six hosts. Published-head CI remains a separate gate.

See the [native review](../../../docs/changes/rust-chemical-naming.md) for
reproduction steps, image captions, signature/source identities and declared
coverage. `sha256.json` lists raw evidence bytes; authored checkers and this
README are intentionally outside that raw-artifact manifest.
