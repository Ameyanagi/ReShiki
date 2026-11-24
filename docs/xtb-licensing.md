# xTB: licensing and packaging

> **Status: draft wording, for outside review before the first release with
> xTB.** This covers the LICENSE section, NOTICE, `licenses/axtb/`, the
> `reshiki --version` and About text, README.txt in the archives and the
> source-archive directions. Remove this note only after that review.

ReShiki can run xTB calculations (GFN2-xTB first) with **axtb**, a Rust
implementation of the xTB methods licensed under the GNU Lesser General Public
License, version 3 or later (LGPL-3.0-or-later). ReShiki uses its own fork,
<https://github.com/Ameyanagi/axtb>, branch `reshiki`. This page records how the
optional LGPL component is built, shipped and audited. The worker protocol is
in [xTB worker](xtb-worker.md).

## Which builds contain axtb

| Product                                          | axtb inside | How                                             |
| ------------------------------------------------ | ----------- | ----------------------------------------------- |
| macOS and Linux desktop releases (and nightlies) | yes         | `build_release.py` builds with `--features xtb` |
| Windows desktop releases                         | no          | until Windows code signing works                |
| `cargo build` / `cargo run` (default features)   | no          | the `xtb` feature is off by default             |
| Python wheel `reshiki`                           | no          | `crates/python` never enables it                |
| npm packages and the web editor (WebAssembly)    | no          | the web crates never enable it                  |
| Nix flake                                        | no          | builds default features                         |
| Planned Python package `reshiki-xtb`             | yes         | separate LGPL distribution, see below           |

Only the root application crate has the Cargo feature `xtb`. It turns on the
`engine` feature of `crates/xtb-worker`, the only crate that calls axtb. Library
crates never enable either feature. Without the feature `crates/xtb-worker`
still builds its protocol types and client (MIT OR Apache-2.0, no axtb code),
and `reshiki --xtb-worker` answers that xTB is not included.

axtb is statically linked into the `reshiki` executable. Calculations run in a
child process of that same executable (`reshiki --xtb-worker`), for isolation
(deadlines, memory, crashes), not as a licensing boundary. There is no separate
helper executable on the desktop.

## The pinned revision

The fork revision is written once, in the root `Cargo.toml`
(`[workspace.dependencies] axtb-driver`). `Cargo.lock` records the same
revision. The dependency audit checks the other copies against `Cargo.lock`:
`reshiki_xtb_worker::AXTB_REV` (shown by `--version`, About and the worker's
`hello`), and `licenses/axtb/SOURCE`. To move the pin, change `Cargo.toml`, run
`cargo update -p axtb-driver`, and update the two copies the audit checks.

The fork's workspace patches two `gemm` crates (`third_party/README.md` in the
fork) to cut start-up time on machines with about 100 or more CPU threads.
Cargo ignores a dependency's `[patch]` sections, so ReShiki builds the unpatched
crates.io versions. Desktop machines have few enough threads that the cost is
small, and keeping our patch list free of these copies keeps the source story
simple. Reconsider if a measurement on a large Linux workstation shows a
visible delay.

## License texts and notices

- `LICENSE` scopes ReShiki's MIT OR Apache-2.0 grant to original ReShiki code
  and has a section on the optional LGPL component.
- `NOTICE` lists `licenses/axtb/`.
- `licenses/axtb/` holds `COPYING` (GPL-3.0 text), `COPYING.LESSER`
  (LGPL-3.0 text), axtb's `NOTICE` (its upstream work, including the g-xTB
  authors' permission), and `SOURCE` (fork URL, branch, revision, and where the
  matching source archive is published).
- `scripts/license_notices.py` writes `THIRD-PARTY-NOTICES.txt` from the
  dependency graph of the build being packaged. It includes `licenses/axtb/`
  only when that graph contains the fork's crates, so default builds and the
  Python wheel carry no LGPL text they do not need. In an xtb build each axtb
  crate also contributes its own `COPYING`, `COPYING.LESSER` and `NOTICE`.

## Licence audit

`scripts/check_agent_dependencies.py` fails when:

1. the default-feature graph (normal and build edges) of any workspace package,
   for any release target or `wasm32-unknown-unknown`,
   contains a package bound by a GPL-family license (GPL, LGPL, AGPL, in any
   form), or any package from the axtb fork. An `OR` expression with a
   non-GPL alternative, such as `self_cell`'s `Apache-2.0 OR GPL-2.0-only`, is
   used under that alternative and passes; a parenthesized expression with any
   GPL-family term fails;
2. the set of GPL-family packages in the application's `--features xtb` graph
   for the macOS and Linux release targets differs from the seven fork crates
   at the pinned revision (`axtb`, `axtb-base`, `axtb-d4`, `axtb-driver`,
   `axtb-gfnff`, `axtb-gxtb`, `axtb-solvation`), or any of those declares a
   license other than LGPL-3.0-or-later;
3. any new package in the xtb graph has a license outside the permissive list
   (the existing rule for new dependencies).

The historical baseline exemption does not apply to rules 1 and 2. The audit
also requires `AXTB_REV`, `Cargo.lock` and `licenses/axtb/SOURCE` to name the
same revision, and `licenses/axtb/` texts to match the pinned crates byte for
byte.

## Corresponding source

Each release that contains axtb also publishes
`reshiki-<version>-source.tar.gz` (and its `.sha256`) on the same GitHub
release page. `scripts/xtb_source_archive.py` builds it from the release
commit:

```
reshiki-<version>-source/
  …                     ReShiki's tracked files at the release commit (git archive)
  axtb-source/          the fork's tracked files at the pinned revision
  vendored-crates/      `cargo vendor --locked` of every dependency
  .cargo/config.toml    source replacement to vendored-crates/
  BUILD-WITH-XTB.md     build and rebuild steps
  SOURCE-MANIFEST.json  commits, revision, and SHA-256 of every file
```

The release workflow publishes binaries only after this archive is built,
checked and uploaded. The archive is kept as long as the release it belongs
to; nightly releases that ship axtb carry their own archive.

`BUILD-WITH-XTB.md` gives two procedures, offline:

- **Rebuild as shipped:**
  `cargo build --profile dist --locked --offline --features xtb --bin reshiki`.
- **Rebuild with a modified axtb:** edit files under `axtb-source/`, then build
  with the fork's crates patched to those paths
  (`--config 'patch."https://github.com/Ameyanagi/axtb".axtb-driver.path="axtb-source/crates/driver"'`,
  one entry per fork crate, written out in the file) and without `--locked`.
  The result is a `reshiki` executable that uses the modified library. On
  macOS, sign it ad hoc (`codesign --force --sign - …`) or wrap it as described
  in the file; the system may ask once before running an unsigned app.

`xtb_source_archive.py check` repeats the second procedure in a clean
directory with a one-line change to axtb (its reported program version) and
requires the rebuilt worker to answer `hello` with the changed version and a
water energy. The release workflow runs this check on Linux x64 before
publishing.

LGPL-3.0 section 4(d)(0) is met by giving the complete source of the combined
work (ReShiki is itself open source) with the steps to rebuild it against a
modified axtb. ReShiki's desktop builds are not "User Products" in the GPLv3
sense, so no installation keys are involved.

## What users see

- `reshiki --version` prints a second line:
  `xTB: included (axtb 0.2.0, LGPL-3.0-or-later, source https://github.com/Ameyanagi/axtb/tree/<rev>)`
  or `xTB: not included in this build`.
- The About window shows the same fact in its credits line, with a link to the
  source.
- The archive's README.txt says that the build contains axtb under the LGPL and
  where the matching source archive is.
- `build.json` records `xtb: {included, engine, version, license, source, rev,
source_archive}`.

## Python: the planned `reshiki-xtb` distribution (design only)

The `reshiki` wheel stays MIT OR Apache-2.0 and never contains axtb. A separate
distribution adds calculations for those who opt in:

- **`reshiki-xtb`** (LGPL-3.0-or-later as a whole): import package
  `reshiki_xtb`, containing one executable, `reshiki-xtb-worker`, built from
  `crates/xtb-worker` with its `engine` feature (a `[[bin]]` with
  `required-features = ["engine"]`). It speaks the same worker protocol. The
  wheel carries the axtb texts and a source notice like the desktop archive.
- The `reshiki` wheel keeps the MIT client from `crates/xtb-worker` (no axtb
  code). It finds the worker by importing `reshiki_xtb` and asking for
  `reshiki_xtb.worker_path()`; it never searches `PATH`. A missing or
  incompatible worker is reported as "xTB not installed" or "xTB version
  mismatch", never as a crash. It checks the worker's `hello` (protocol and
  version) before exposing calculation functions.
- `pip install "reshiki[xtb]"` installs both: the extra pins
  `reshiki-xtb==<same version>`.
- Wheels: platform-tagged (`macosx_14_0_arm64`, `macosx_14_0_x86_64`,
  `manylinux_2_28_x86_64`, `manylinux_2_28_aarch64`, the same floors as the
  `reshiki` wheel); no Windows wheel until Windows signing works. The
  executable bit is checked after install.
- Publication order: `reshiki-xtb` to TestPyPI first; install the candidate
  pair from a local wheelhouse (`--no-index --find-links`) and run a water
  energy; then publish `reshiki-xtb` to PyPI and verify it installs; only then
  publish the `reshiki` wheel that declares the extra. The base wheel's tests
  never run `uv sync --all-extras` against the index, so they cannot pick up an
  already published `reshiki-xtb` and hide a broken candidate.
- Tests: base-only install (no calculation functions, clear message), each
  supported platform, an unsupported platform (extra resolves to nothing,
  message), version skew (refused), and removal of `reshiki-xtb` after use.

## Release gates for the first release with xTB

- Outside review of the wording on this page, then remove the status note.
- The dependency audit confirms that `licenses/axtb/SOURCE`, `AXTB_REV` and
  `Cargo.lock` agree and the three copied license and notice files match axtb.
- The source archive is built, checked (`xtb_source_archive.py check`) and
  uploaded before any binary.
- The packaged macOS and Linux executables answer a water energy through
  `--xtb-worker`; the Windows executable answers that xTB is not included.
