# Controlled Office SVG review evidence

These are original controlled O–SiMe₃ inputs and exports, actual Mac Office documents, unmodified application screenshots, and immutable source/build/test records. No unrelated user document or proprietary application binary is included. Original contributed code, tests, documentation and authored chemical fixtures are offered under **MIT OR Apache-2.0**, following [CONTRIBUTING.md](../../../../CONTRIBUTING.md); screenshots retain the displayed applications' UI.

Run with Python 3 and no additional packages:

```sh
python3 docs/changes/fixtures/office-svg-placement/verify.py
# Optional: compare this checkout's production/test source with the preserved build.
python3 docs/changes/fixtures/office-svg-placement/verify.py --source-root .
```

The verifier checks all 53 copied evidence-file hashes; exact final ordinary/Office/native identity pairs; native graph and SiMe₃ abbreviation; physical SVG extents; the Office picture's referenced SVG relationship, text/path content, extent, offset, crop and rotation; source correction scope; and the actual screenshot producer identities. Seven controls reject retained text, doubled SVG width, a changed chemical bond, an incorrect abbreviation anchor, a one-EMU size change, cropping and relabeling earlier screenshots as final-app captures. It verifies the preserved record rather than replaying desktop actions.

[Provenance](provenance.json) and [raw evidence manifest](evidence-manifest.json) retain exact identities. The full source/build dictionaries distinguish 533 compiled inputs from 1,061 Rust/manifest inputs. The first production build rebuilt all 15 local artifacts from this worktree. The final application target was freshly compiled under the same exclusive lease, using unchanged local libraries; the only changed production input is `src/app/inspector.rs` for two display labels. The menu regression is a separate test-source delta. Source commit is `3d261b32e9baa73b6d2cb133de2163fa3699569a`.

Before producer: signed historical ReShiki `493a0cb4…`; compiler mapping to declared `51fa0991` is **unverified**. Actual Office screenshots/documents use the first candidate `a5267295…`. Final native menu/export screenshots use `3d343a93…`; its exported SVG and native bytes match the earlier actual Office input exactly. The before/after Office extent is exactly 1,104,900 × 368,300 EMU; this differs slightly from authored point dimensions. The precise Office rounding algorithm is not established. The review page records capture-state differences, the untouched Recovery draft banner, and the successful save/reopen checks.

The formatting configuration excludes only the raw `final-candidate-export-receipt.json` from Oxfmt. This preserves its original SHA-256 `327d3777f9e87f6cbecb8db8d5f5706f6aa6694490c7c96dc9a8b91f03e4dc8c`; all other matched package files are formatted normally.

The initial workspace check was interrupted after observed compiler inactivity. Its log/receipt is retained, followed by the successful incremental-disabled retry. No compiler/cache root cause is claimed. Full workspace checks precede the final label-only correction; the final label patch has its own accessible-widget test, formatting, production Clippy and build records. This bundle makes no Windows Office desktop or full #64 completion claim.

Review and reusable raw images: [Office SVG placement](../../office-svg-placement.md). Under review; [draft PR #290](https://github.com/Ameyanagi/ReShiki/pull/290). Contributor: @Ameyanagi, project creator and maintainer.
