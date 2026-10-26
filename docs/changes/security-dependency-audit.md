# Dependency security review, 26 October 2026

This review supports [the release security gate, #585](https://github.com/Ameyanagi/ReShiki/issues/585).
It covers the locked Rust, JavaScript and Python graphs, selected Nix libraries,
and CI runtimes. It is a dated advisory and
reachability assessment, not a claim that every component is vulnerability-free.

The initial OSV query checked 1,225 distinct ecosystem/name/version records from
620 Cargo packages, 602 Bun records and seven Python records. One Bun workspace
alias has no registry version. The API returned 21 matching packages and 74
advisory identifiers; six matched packages carry maintenance-only notices.
Patched Bun's own audit independently identified the same nine affected npm
packages. Raw query/response receipts and the reproducible standard-library
scanner are retained in `evidence/security-2026-10-26/dependency-audit` in the
release evidence workspace.

## Updated dependencies

| Dependency                         | Before   | Updated    | Advisory scope                                                                                                             |
| ---------------------------------- | -------- | ---------- | -------------------------------------------------------------------------------------------------------------------------- |
| rustls                             | 0.23.45  | 0.23.46    | RUSTSEC-2026-0346 through 0349: session Debug secrets, ECH handling, optional Brotli allocation and key-exchange downgrade |
| rustls-webpki                      | 0.103.15 | 0.103.16   | [RUSTSEC-2026-0382](https://rustsec.org/advisories/RUSTSEC-2026-0382.html): reject distrusted emSign intermediates         |
| webpki-root-certs                  | 1.0.9    | 1.0.10     | [RUSTSEC-2026-0383](https://rustsec.org/advisories/RUSTSEC-2026-0383.html): remove the emSign root                         |
| tokio                              | 1.53.1   | 1.53.3     | [RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html): AsyncFd I/O safety                             |
| fast-uri                           | 3.1.8    | 3.1.9      | Four URI normalization/host/scheme advisories                                                                              |
| postcss-selector-parser            | 6.1.4    | 7.1.6      | Selector parsing CPU exhaustion                                                                                            |
| sharp                              | 0.35.4   | 0.35.5     | Bundled librsvg CVE-2026-96889                                                                                             |
| smol-toml                          | 1.8.0    | 1.9.0      | Quadratic parsing                                                                                                          |
| source-map-js                      | 1.2.1    | 1.2.2      | Indexed source-map denial of service                                                                                       |
| undici                             | 8.10.2   | 8.12.2     | EventSource allocation, SRI and content-encoding errors                                                                    |
| vite                               | 8.3.0    | 8.3.3      | Six dev-server/editor/file-access advisories                                                                               |
| zod                                | 4.6.5    | 4.6.6      | Oversized constrained collection allocation                                                                                |
| Pillow, development reference only | 12.3.0   | 13.0.0     | 41 image/reference package advisories                                                                                      |
| certifi, development CI TLS helper | absent   | 2026.10.23 | Updated CA bundle; required minimum 2026.10.19                                                                             |

The npm security overrides are exact versions; unchanged direct framework
versions remain locked. The selector parser upgrade crosses a major version and
requires the website checks/build in addition to the lock audit. Python's
Pillow constraint now permits 13.x; certifi is a development dependency.

Rust HTTPS is desktop-only and uses reqwest with default features disabled and
`rustls` enabled. Neither reqwest Brotli decoding nor rustls's non-default
Brotli certificate feature is in the locked graph. There is no Google C Brotli,
`brotli-sys`, Rust `brotli` or `woff2` package in Cargo.lock. The Rustls optional
Brotli allocation advisory is distinct from Google's C decoder CVE-2026-111497.
The Linux verifier uses native OS roots with webpki validation; Apple and
Windows use their platform verifiers. `webpki-root-certs` is a wasm-only
dependency of rustls-platform-verifier 0.7.0, while this application's reqwest
dependency is desktop-only. Its lock entry is nevertheless updated.

## Scoped dispositions, not package-level fixes

**lru 0.16.4 / [RUSTSEC-2026-0253](https://rustsec.org/advisories/RUSTSEC-2026-0253.html).**
The issue requires `LruCache::pop()`, a stored key whose destructor can panic,
and recovery from that unwind before another cache operation. The sole locked
consumer is cryoglyph 0.1.0's glyph atlas. Its key is cosmic-text 0.15.0's
`CacheKey`, which derives `Copy` and cannot have a destructor. The reviewed
atlas calls `get`, `get_or_insert`, `peek_lru` and `pop_lru`, not `pop`. These
preconditions are absent in the current application. The component remains in
the affected version range; no fixed-version claim is made. The guard pins the
reviewed lru/cryoglyph/cosmic-text source checksums and rejects another consumer
or a changed key implementation until reviewed.

**http-cache-semantics 4.2.0 / [CVE-2026-93748](https://github.com/advisories/GHSA-ch52-4w7c-c8xp).**
No fixed version was published in the advisory at review time. Its only locked
consumer is Astro 7.3.3. Astro's image-build `assets/build/remote.js` constructs
its own unauthenticated image requests and uses `storable()` and `timeToLive()`;
it never invokes the vulnerable client `max-stale` cache-hit path. Revalidation
supplies only the image ETag/Last-Modified conditions. The site is static and
has no authenticated shared-response cache. The dependency remains affected in
other uses. The guard requires the reviewed Astro archive integrity and sole
consumer, and explicit static output without a server adapter. Its reviewed
remote-image module SHA-256 is
`f373fa76e3112446db327c79b34e2bbb1ef1dcad41affb60788adf30edc9588e`.

**Pillow 13.0.0's bundled native font libraries.**
Verified macOS ARM64 and Linux x64 wheels still contain Google's unpatched
Brotli 1.2.0 via FreeType. The Pillow package update does **not** remove or fix
that embedded CVE-2026-111497 decoder. The affected route is WOFF2/font decoding,
which the current Python reference tools do not invoke: they perform raster
pixel operations, and the embedded-picture reference accepts only PNG, TIFF,
JPEG, GIF and BMP. There are no project calls/imports of Pillow ImageFont,
ImageDraw, `_imagingft`, RDKit Draw/Cairo or their direct dynamic-import forms.
The AST guard rejects adding those entry points without reassessment. This is
a current-use disposition for development/reference tools; these Python wheels
are not shipped in the native application. Native system/Nix font libraries
have a separate audit and cannot be cleared by this Python usage guard.

Maintenance-only notices for paste, rustybuzz, ttf-parser, zbus_names, zvariant
and zvariant_derive remain visible. They are not treated as repaired CVEs or
silently suppressed by version changes. Replacing the rendering/desktop stack
requires its own compatibility review.

## Regression and verification contract

`scripts/check_dependency_security_scope.py` checks the dated fixed-version
baselines and the scoped conditions above. The standard Python test discovery
includes its tests, which reject font API imports/aliases, a new LRU/cache
consumer, changed reviewed sources and server rendering. The guard does not
replace a fresh advisory scan or resist deliberately rewritten source code.

Security dependency changes have no intended visible UI change. Validation
must include the locked Cargo tests/checks, website check/build after the parser
upgrade, JavaScript regressions, and Pillow-backed reference image comparisons.
Record their exact results in the release evidence. Continue to report raw
scanner matches together with these dispositions; do not label an eight-match
postscan as “zero advisories.”

The final local scan queried 1,226 distinct versions after adding certifi. It
retained eight raw matches: the six maintenance notices and the two scoped
exposure findings described above, with no new blocking finding or failed
scope guard. The Checks workflow now repeats this scan once per run against
the current OSV database. Exact advisory ID, package, version, source and
metadata revision must match the reviewed policy; a known alias does not
exempt a new advisory. Maintenance notices also require fresh RustSec source
metadata. Network errors, partial responses and changed classifications fail
the job. Raw reports and lock hashes remain available as workflow artifacts,
including after a scan failure when the runner runtime check has passed.

Local validation passed 3,128 workspace library/binary tests, five independent
image-reference comparisons and nine Python picture tests. Website checking
and the build passed after the selector parser upgrade. The scanner, runtime
guard and TLS helper passed 22 focused regression tests, including their
failure paths. Cross-platform CI, Nix builds and final release package checks
must still pass on the final source before publication.

## CI runtimes and action dependencies

[Node's 21 October security release](https://nodejs.org/en/blog/vulnerability/october-2026-security-releases)
fixes CVE-2026-111497, the Google C Brotli decoder memory corruption issue,
alongside DNS, TLS and HTTP issues. Workflow commands now select Node 22.23.4
or 24.22.1 explicitly. Bun is pinned to 1.4.4, the
[patched Bun release](https://github.com/oven-sh/bun/security/advisories/GHSA-663w-cv8g-jj2p).
The app does not ship either runtime.

JavaScript actions run with the runner's bundled Node, independently of
`setup-node`. Every repository-owned job therefore starts with the same
PowerShell guard before checkout or another JavaScript action. It finds the
live `Runner.Worker`, verifies its sibling Node executable and requires stable
Node 24.22.1 or newer within Node 24. Missing, ambiguous or older installations
fail the job. It never substitutes a `PATH` executable or changes the runner's
installation. The canonical script and embedded workflow copies are checked
for equality. Node 24.22.1 backports the decoder fix while still reporting
Brotli 1.2.0, so the guard checks the patched Node release rather than rejecting
that version label.

The reviewed direct and transitive action manifests are recorded in
`.github/actions/runtime-manifests.json`. Remote actions are pinned to full
commit hashes and JavaScript actions use Node 24. Policy tests reject unknown
action pins and action pre-hooks that would precede the guard. Pages actions
and setup-uv were upgraded to the reviewed versions. Runner-internal work
before the first step remains GitHub's responsibility. The sampled earlier
jobs used runner 2.338.0 with Node 24.21.0; they would fail this new check.
[Runner 2.338.1](https://github.com/actions/runner/releases/tag/v2.338.1)
contains the patched runtime. A successful guarded job, not the existence of
that upstream release, is the evidence required for the release gate.

Some current upstream action bundles still include affected Undici versions.
Updating runner Node does not replace those bundles. The reviewed HTTP client
uses Undici's proxy agent, while `@actions/github` also uses its fetch API for
fixed GitHub endpoints. No EventSource use or attacker-controlled response
endpoint was found in these workflow call paths. This bounds the current use
of the [EventSource advisory](https://github.com/nodejs/undici/security/advisories/GHSA-4qv8-2p9j-2685)
and [content-encoding advisory](https://github.com/nodejs/undici/security/advisories/GHSA-xjj6-f33q-929c);
it does not declare the bundled dependencies patched. The repository's own
Undici dependency is updated separately in the table above.

## Python build tooling and certificate trust

Workflows select managed CPython 3.12.15 and uv 0.13.5. The managed-only setting
prevents a cached older Python from satisfying the selection. Python is used
for build and reference checks and is absent from distributed app packages.
All project Python HTTPS calls use `scripts/download_tls.py`, which requires
certifi 2026.10.19 or newer and constructs a hostname-verifying TLS context
from that bundle alone. It never adds operating-system roots or falls back to
them. The lock selects certifi 2026.10.23. Existing download size and checksum
checks remain in place; offline packaging imports still work without certifi.

[Python 3.12.16](https://www.python.org/downloads/release/python-31216/)
was released as source on 25 October. Its binaries were not yet available in
the reviewed uv or actions/python-versions catalogs. This change does not
claim to install its interpreter fixes. The explicit updated CA bundle and
patched uv address this project's download trust path for the compromised
eMudhra intermediates. Reviewed XML inputs are trusted reference/test data;
the project does not directly import `mimetypes`; release extraction uses the
`data` filter rather than `tar_filter`, and source archives are checksum-pinned.
These are current-use limits for the remaining interpreter advisories, not
equivalents of installing Python 3.12.16. Reassess when the managed build is
available or those input paths change.

## Nix and native library scope

The Nix lock refresh selects OpenSSL 3.5.9. The flake's shared overlay pins
Google Brotli 1.2.1 and curl 8.23.0, and applies three upstream FreeType fixes
for CVE-2026-50811 and CVE-2026-95512 on top of the existing Nix patches.
Source hashes and patch hashes are fixed in `flake.nix`. All three supported
Nix systems evaluate with the overlay; actual build and installed closure
validation remains the Nix CI job's responsibility.

This review does not clear every package in the nixpkgs catalog, every host OS
library, or libraries dynamically loaded by the host graphics stack. Portable
package import inspection found only the expected platform libraries, but
that is not a complete host OS vulnerability scan. Keep operating systems
updated separately. Native font libraries also cannot be cleared by the
Pillow reference-tool disposition above.

## Certificate-data license continuity

The `webpki-root-certs` update retains its existing CDLA-Permissive-2.0 terms. Versions 1.0.9 and 1.0.10 ship byte-identical agreement text (SHA-256 `e271993808fec50ab29350b39539cdec611a9103f827e0aa26d61da70e2d33f8`). The older version was already in the historical dependency baseline; the new version required an explicit review. The policy now accepts only the reviewed 1.0.10 crates.io archive, matched by name, version, license, source and checksum. The global license allowlist and historical baseline remain unchanged. [CDLA-Permissive-2.0](https://cdla.dev/permissive-2-0/) permits sharing the data with the full agreement supplied and imposes no restrictions on results. Release notice generation preserves the complete agreement, original manifest and review record in `Licenses/THIRD-PARTY-NOTICES.txt`; no separate supplement or relabeling of upstream terms is needed.
