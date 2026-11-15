---
section: Changes
issues: []
---

- **Breaking: the headless JavaScript library moves to its own npm package,
  `@ameyanagi/reshiki-headless`.** To migrate, run
  `bun add @ameyanagi/reshiki-headless` and import from
  `"@ameyanagi/reshiki-headless"` instead of `"@ameyanagi/reshiki"` (and load
  `@ameyanagi/reshiki-headless/reshiki_web_bg.wasm` instead of
  `@ameyanagi/reshiki/reshiki_web_bg.wasm`); the functions are unchanged.
  `@ameyanagi/reshiki` now holds only the editor and its worker, so an install
  no longer downloads WebAssembly it does not use: the editor package is 15.1
  MB instead of 20.6 MB, and the headless package is 6.4 MB.
  `@ameyanagi/reshiki/editor`, `/element`, `/embed`, `/frame.html` and
  `/reshiki_bg.wasm` stay where they are. See
  [Moving from 0.15](../packages/headless/README.md#moving-from-015).
- **Developer.** Published builds use a new Cargo `dist` profile: the
  `release` profile with fat link-time optimization and one codegen unit. The
  macOS builds use thin link-time optimization, because the fat-LTO link needs
  more memory than the macOS arm64 runner has. With the changes below, the
  macOS arm64 executable goes from 81.4 MB to 72.6 MB and the portable archive
  from 33.8 MB to 32.7 MB. `scripts/build_release.py` and the Release builds
  workflow now read the binary and its symbol file from
  `target/<target>/dist/`; `cargo build --release`, tests and development
  bundles are unchanged. See [Build locally](releasing.md#build-locally).
- **Developer.** The layout template catalog, the bundled drawing templates
  and the NMR shift index are stored compressed in the executable and inflated
  on first use: 2.66 MB of embedded data becomes 0.50 MB. Downloads stay about
  the same, because archives already compressed this data.
- **Developer.** The update check and online name lookup use ring for TLS
  instead of aws-lc. The executable is almost 2 MB smaller and the build no
  longer needs CMake. These connections no longer offer the post-quantum
  hybrid key exchange, which rustls provides only with aws-lc.
