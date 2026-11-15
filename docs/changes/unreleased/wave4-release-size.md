---
section: Developer
issues: []
---

- Published builds use a new Cargo `dist` profile: the `release` profile with
  fat link-time optimization and one codegen unit. On macOS arm64 the
  executable shrinks from 77.9 MB to 67.8 MB and the portable archive from
  33.5 MB to 31.3 MB. `scripts/build_release.py` and the Release builds workflow
  now read the binary and its symbol file from `target/<target>/dist/`;
  `cargo build --release`, tests and development bundles are unchanged. See
  [Build locally](releasing.md#build-locally).
- The layout template catalog, the bundled drawing templates and the NMR shift
  index are stored compressed in the executable and inflated on first use:
  2.66 MB of embedded data becomes 0.50 MB. Downloads stay about the same,
  because archives already compressed this data.
- The update check and online name lookup use ring for TLS instead of aws-lc.
  The executable is almost 2 MB smaller and the build no longer needs CMake.
  These connections no longer offer the post-quantum hybrid key exchange,
  which rustls provides only with aws-lc.
