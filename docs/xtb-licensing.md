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
documented in `docs/xtb-worker.md`, which arrives with the worker. The fork's
numerical validation on macOS and Linux is in its
[validation report](https://github.com/Ameyanagi/axtb/blob/reshiki/docs/reshiki/validation.md).
g-xTB is on: the fork's `gxtb` feature is on by default and ReShiki keeps it.

## Which builds contain axtb

| Product                                          | axtb inside | How                                             |
| ------------------------------------------------ | ----------- | ----------------------------------------------- |
| macOS and Linux desktop releases (and nightlies) | yes         | `build_release.py` builds with `--features xtb` |
| Windows desktop releases                         | no          | until Windows code signing works                |
| `cargo build` / `cargo run` (default features)   | no          | the `xtb` feature is off by default             |
| Python wheel `reshiki`                           | no          | `crates/python` never enables it                |
| npm packages and the web editor (WebAssembly)    | no          | the web crates never enable it                  |
| Nix flake                                        | no          | builds default features                         |
| Python package `reshiki-xtb`                     | yes         | separate LGPL distribution, see below           |

Only the root application crate has the Cargo feature `xtb`. It turns on the
`engine` feature of `crates/xtb-worker`, the only crate that calls axtb. Library
crates never enable either feature. The separate Python wheel builds the
worker executable with `engine`. Without the feature `crates/xtb-worker`
still builds its protocol types and client (MIT OR Apache-2.0, no axtb code),
and `reshiki --xtb-worker` answers that xTB is not included.

axtb is statically linked into the `reshiki` executable. Calculations run in a
child process of that same executable (`reshiki --xtb-worker`), for isolation
(deadlines, memory, crashes), not as a licensing boundary. There is no separate
helper executable on the desktop.

## The pinned revision

The fork revision is written in the root `Cargo.toml`: the
`[workspace.dependencies] axtb-driver` entry and the two `gemm` patches below.
`Cargo.lock` records it. The dependency audit checks every other copy against
`Cargo.lock`: the two patches, `reshiki_xtb_worker::AXTB_REV` (shown by
`--version`, About and the worker's `hello`), and `licenses/axtb/SOURCE`. To
move the pin: change the three entries in `Cargo.toml`, run
`cargo update -p axtb-driver`, update `AXTB_REV` and `licenses/axtb/SOURCE`,
and copy the fork's `COPYING`, `COPYING.LESSER`, `NOTICE` and
`CHANGES-reshiki.md` into `licenses/axtb/` (the audit compares them byte for
byte).

The fork's workspace patches two `gemm` crates, `gemm-common` and
`private-gemm-x86` (`third_party/README.md` in the fork), so they read one CPU's
cache description instead of every CPU's. Cargo ignores a dependency's `[patch]`
sections, so ReShiki's root `Cargo.toml` repeats them, taking the patched copies
from the fork at the same revision, so ReShiki builds the engine and `gemm`
sources the fork validated (other dependency versions follow ReShiki's own
lockfile). Only the xtb graph uses `gemm`. The copies are MIT and keep their own
`LICENSE` files, which the notices include with the fork as their source.

## License texts and notices

- `LICENSE` scopes ReShiki's MIT OR Apache-2.0 grant to original ReShiki code
  and has a section on the optional LGPL component.
- `NOTICE` lists `licenses/axtb/`.
- `licenses/axtb/` holds `COPYING` (GPL-3.0 text), `COPYING.LESSER`
  (LGPL-3.0 text), axtb's `NOTICE` (its upstream work, including the g-xTB
  authors' permission), `CHANGES-reshiki.md` (the fork's notice of
  modification under GPLv3 section 5(a)), and `SOURCE` (fork URL, branch,
  revision, and where the matching source archive is published).
- `scripts/license_notices.py` writes `THIRD-PARTY-NOTICES.txt` from the
  dependency graph of the build being packaged. It includes `licenses/axtb/`
  only when that graph contains the fork's crates, so default builds and the
  base Python wheel carry no LGPL text they do not need. In an xtb build each axtb
  crate also contributes its own `COPYING`, `COPYING.LESSER` and `NOTICE`.

## Licence audit

`scripts/check_agent_dependencies.py` fails when:

1. the default-feature graph (normal and build edges) of any workspace package,
   for any release target or `wasm32-unknown-unknown`,
   contains a package bound by a GPL-family license (GPL, LGPL, AGPL, in any
   form), or any package from the axtb fork. An `OR` expression with a
   non-GPL alternative, such as `self_cell`'s `Apache-2.0 OR GPL-2.0-only`, is
   used under that alternative and passes. Expressions are read with SPDX
   precedence (`WITH`, then `AND`, then `OR`, parentheses respected): an `AND`
   with a GPL-family part fails;
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
release page. `scripts/xtb_source_archive.py` packages the tracked working-tree
files from the release checkout. For nightlies, the source and desktop build
jobs stamp the same version into `Cargo.toml` and `Cargo.lock` first.

The archive is for building and testing the ReShiki application and the
`reshiki-xtb` Python wheel. It excludes
`docs/images/` assets used by the website, so building the website requires
the full repository.

```
reshiki-<version>-source/
  …                     ReShiki's tracked working-tree files, with media exclusions
  axtb-source/          the fork's tracked files at the pinned revision
  vendored-crates/      `cargo vendor --locked` of every dependency
  .cargo/config.toml    source replacement to vendored-crates/
  BUILD-WITH-XTB.md     build and rebuild steps
  SOURCE-MANIFEST.json  commits, revision, exclusions, and SHA-256 of every file
```

The release workflow publishes binaries only after this archive is built,
checked and uploaded.

**Each published binary has its source available for as long as the binary
is.** The archive is an asset of the same GitHub release as the binaries it
corresponds to, uploaded by the same `gh release create` call (stable releases
in `release.yml`, nightlies in `nightly.yml`). The GitHub CLI publishes the
release only after every asset has uploaded, so a published release never has
one without the other. Deleting a release deletes all of its assets, so the
archive and its binaries disappear together:

- Tagged releases keep their archive permanently.
- Nightlies are pruned with their binaries. No workflow or script deletes
  nightly releases today (the oldest still published dates from
  2026-09-29), so a nightly's archive stays as long as its nightly does. When a
  nightly is removed, by hand or by a future pruning step, it must be removed
  as a whole release (`gh release delete`), which takes its archive with it.
- Workflows and scripts never add, replace or delete single assets of an
  application release (only the separate `web-demo` release replaces its own
  asset). A test fails on the usual ways of doing so: `gh release upload`,
  `--clobber`, `gh release delete-asset`, and the REST `releases/assets`
  endpoint from the shell or Python.

`BUILD-WITH-XTB.md` lists the release profile settings for macOS, Linux arm64,
and Linux x64, and gives two offline procedures:

- **Rebuild as shipped:**
  `cargo build --profile dist --locked --offline --features xtb --bin reshiki`.
- **Rebuild with a modified axtb:** edit files under `axtb-source/`, then build
  with the fork's crates patched to those paths
  (`--config 'patch."https://github.com/Ameyanagi/axtb".axtb-driver.path="axtb-source/crates/driver"'`,
  one entry per fork crate, written out in the file) and without `--locked`.
  The result is a `reshiki` executable that uses the modified library. On
  macOS, sign it ad hoc (`codesign --force --sign - …`) or wrap it as described
  in the file; the system may ask once before running an unsigned app.

`xtb_source_archive.py check` first runs `cargo check --locked --offline
--features xtb --bin reshiki` in the pristine extracted tree with an empty
`CARGO_HOME`. It then builds with a one-line change to axtb's reported program
version and verifies that the rebuilt worker reports that version and computes
a water energy. The release workflow runs both steps on Linux x64 before
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

## Python wheels for xTB

The `reshiki` wheel stays MIT OR Apache-2.0 and excludes axtb. Calculations
require the matching `reshiki-xtb` wheel.

- **`reshiki-xtb`** (LGPL-3.0-or-later as a whole) provides import package
  `reshiki_xtb` and the executable `reshiki-xtb-worker`, built from
  `crates/xtb-worker` with `engine`. The wheel carries `COPYING`,
  `COPYING.LESSER`, axtb's `NOTICE`, `CHANGES-reshiki.md` and `SOURCE` in its
  consolidated notices. Its matching complete source is
  `reshiki-<version>-source.tar.gz` on
  `https://github.com/Ameyanagi/ReShiki/releases/tag/v<version>`.
- The client finds the installed executable through `reshiki_xtb.worker_path()`.
  It checks the package version and verifies the worker's protocol handshake
  before starting a calculation.
- The `xtb` extra pins `reshiki-xtb==<same version>`.
- Wheels support macOS 14 or later on Apple Silicon and Intel, and manylinux
  x86_64 and aarch64. CI checks that the installed worker is executable.
- Version tags publish `reshiki-xtb` to PyPI after the tag's GitHub release
  carries the corresponding source, and before the base `reshiki` wheels. CI
  tests the wheel pair from a local wheelhouse (`--no-index --find-links`). See
  the [release checklist](releasing.md#publish-a-version).
- To use a modified axtb, edit `axtb-source/` in that archive and follow the
  Python wheel section of `BUILD-WITH-XTB.md`. Maturin forwards the same
  Cargo patch overrides used by the desktop rebuild.

## Release gates for the first release with xTB

- Outside review of the wording on this page, then remove the status note.
- The dependency audit confirms that `licenses/axtb/SOURCE`, `AXTB_REV` and
  `Cargo.lock` agree and the four copied license and notice files match the
  fork.
- The source archive is built, checked (`xtb_source_archive.py check`) and
  uploaded before any binary.
- The packaged macOS and Linux executables answer a water energy through
  `--xtb-worker`; the Windows executable answers that xTB is not included.
