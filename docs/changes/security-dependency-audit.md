# Dependency security updates, 26 October 2026

This dated assessment records the fixes and accepted exceptions for
[PR #586](https://github.com/Ameyanagi/ReShiki/pull/586),
[security #585](https://github.com/Ameyanagi/ReShiki/issues/585) and
[Nix CI #588](https://github.com/Ameyanagi/ReShiki/issues/588).
It covers reviewed locked dependencies and build runtimes, not every native,
bundled or host library.

## Dependency fixes

| Dependency                | Before   | Selected   | Reviewed advisory scope                          |
| ------------------------- | -------- | ---------- | ------------------------------------------------ |
| rustls                    | 0.23.45  | 0.23.46    | RUSTSEC-2026-0346 through 0349                   |
| rustls-webpki             | 0.103.15 | 0.103.16   | RUSTSEC-2026-0382, distrust emSign intermediates |
| webpki-root-certs         | 1.0.9    | 1.0.10     | RUSTSEC-2026-0383, updated root data             |
| tokio                     | 1.53.1   | 1.53.3     | RUSTSEC-2026-0386, AsyncFd I/O safety            |
| fast-uri                  | 3.1.8    | 3.1.9      | URI normalization, host and scheme handling      |
| postcss-selector-parser   | 6.1.4    | 7.1.6      | Selector parsing CPU exhaustion                  |
| sharp                     | 0.35.4   | 0.35.5     | Bundled librsvg CVE-2026-96889                   |
| smol-toml                 | 1.8.0    | 1.9.0      | Quadratic parsing                                |
| source-map-js             | 1.2.1    | 1.2.2      | Indexed source-map denial of service             |
| undici                    | 8.10.2   | 8.12.2     | EventSource, SRI and content-encoding fixes      |
| vite                      | 8.3.0    | 8.3.3      | Dev-server, editor and file-access advisories    |
| zod                       | 4.6.5    | 4.6.6      | Constrained collection allocation                |
| Pillow, reference tools   | 12.3.0   | 13.0.0     | Package/image-codec advisories                   |
| certifi, download helpers | absent   | 2026.10.23 | Updated CA bundle, minimum 2026.10.19            |

The npm overrides and lockfiles select these exact versions. The selector-parser
major update needs the website checks/build; Pillow needs the reference-image
comparisons. Desktop reqwest uses rustls with default features disabled;
Cargo.lock contains no Google C Brotli binding or Rust Brotli decoder.

## Eight retained advisory matches

The recorded post-update OSV scan queried 1,226 distinct Cargo/npm/PyPI versions
and retained these eight matches. They remain visible findings, not repaired
versions. This is a dated release assessment rather than a recurring custom
scanner or source-policy gate.

| Locked package             | Advisory                             | Recorded disposition                                               |
| -------------------------- | ------------------------------------ | ------------------------------------------------------------------ |
| lru 0.16.4                 | RUSTSEC-2026-0253                    | Current glyph-cache use lacks the trigger described below          |
| http-cache-semantics 4.2.0 | GHSA-ch52-4w7c-c8xp / CVE-2026-93748 | Current static image-build use lacks the vulnerable cache-hit path |
| paste 1.0.15               | RUSTSEC-2024-0436                    | Maintenance notice                                                 |
| rustybuzz 0.20.1           | RUSTSEC-2026-0206                    | Maintenance notice                                                 |
| ttf-parser 0.25.1          | RUSTSEC-2026-0192                    | Maintenance notice                                                 |
| zbus_names 4.3.4           | RUSTSEC-2026-0371                    | Maintenance notice                                                 |
| zvariant 5.15.0            | RUSTSEC-2026-0370                    | Maintenance notice                                                 |
| zvariant_derive 5.15.0     | RUSTSEC-2026-0372                    | Maintenance notice                                                 |

The lru issue requires `pop()`, a key destructor that panics, and recovery from
that unwind. The sole reviewed consumer is cryoglyph's glyph atlas; its
cosmic-text cache key is `Copy` and has no destructor. The reviewed calls do not
use `pop()`. Astro's sole reviewed http-cache-semantics use is its static
remote-image build path using TTL APIs, not authenticated shared-response
`max-stale` cache hits. Other uses remain affected. Reassess these dispositions
when the relevant dependency or consumer changes.

## Build runtimes and accepted native-library scope

The selected toolchain is official prebuilt rustc **and Cargo 1.99.1**, with three
platform archive hashes pinned in the flake. Bun 1.4.4, uv 0.13.5 and explicit
Node 22.23.4/24.22.1 pins address the reviewed runtime advisories. JavaScript
actions use the runner's embedded Node, independently of `setup-node`; that
runtime must be patched before actions execute.

Nix's executing client/daemon is separate from application build libraries.
Its HTTP decompression occurs before downloaded content hashes are checked.
The reviewed Determinate Nix 3.24.1 bundles contain curl 8.23.0, Brotli 1.2.1
and OpenSSL 3.6.5; the release fixes the Brotli decoder issue. Runtime selection
and actual package/install checks still require validation on the final source.

The maintainer explicitly accepts stock pinned nixpkgs curl/Brotli/FreeType
build-library advisories. The global overlays are removed so cached dependency
outputs remain reusable. This exception does not clear those libraries or
justify an unpatched executing Node/Nix runtime. Original GTK inputs and full
chemical/PNG install checks remain. The Nix preflight rejects source LLVM/Rust/
Cargo builds while permitting cached outputs; locked inputs and disabled source
fallback keep a failed cache download from starting a compiler build.

Inspected Pillow 13 wheels retain Brotli 1.2.0 through font libraries. Current
reference tools perform raster operations and do not decode fonts/WOFF2. That is
a dated use assessment, not a fix to the bundled decoder or an AST-enforced
policy. Python 3.12.15 selection also does not claim Python 3.12.16 fixes.
HTTPS download helpers use the locked certifi CA bundle alone, excluding the
compromised emSign G3 root; the distinct G1 root remains. Offline packaging
continues to use only the standard library.

## Evidence and license continuity

Original scan responses, lock identities and review receipts remain in the
release workspace under `evidence/security-2026-10-26/dependency-audit` and its
hosted-audit evidence. The full earlier assessment and removed scanner/policy
sources are preserved externally under
`evidence/security-2026-10-26/minimal-review/original-b9c7941e`.

Historical validation includes 3,128 native tests, 206 JavaScript/WASM/desktop
comparisons and website/license checks. The earlier prebuilt-toolchain Linux
candidate passed a full flake check in 393.45 seconds using retained caches;
that is not a pristine-cold measurement or validation of subsequent source
changes. Required final-source CI and package checks remain release gates.

The webpki-root-certs update retains CDLA-Permissive-2.0. Its existing exact-source
license exception and review record remain; versions 1.0.9 and 1.0.10 contain the
same agreement. Generated notices preserve the full upstream agreement and
manifest. Maintenance notices, scoped exposures and accepted build libraries
must not be reported as a universal CVE-free inventory.
