# reshiki-xtb

`reshiki-xtb` provides the **axtb** engine, licensed under LGPL-3.0-or-later,
for calculations through `reshiki.xtb`. Install both packages with
`pip install "reshiki[xtb]"`. The engine wheel targets Windows x64 and ARM64, macOS arm64 and x86_64, and
Linux x86_64 and aarch64. On Windows the wheel supplies `reshiki-xtb-worker.exe`.

The wheel carries the complete `COPYING` and `COPYING.LESSER` license texts,
axtb's `NOTICE`, `CHANGES-reshiki.md`, and `SOURCE` in its consolidated notices.
The complete corresponding source is `reshiki-<version>-source.tar.gz` on the
[GitHub release of the same version](https://github.com/Ameyanagi/ReShiki/releases/tag/v0.17.1).
For version `<version>`, the release address is
`https://github.com/Ameyanagi/ReShiki/releases/tag/v<version>`.
The archive includes ReShiki, the pinned axtb fork, every locked Cargo dependency,
and `BUILD-WITH-XTB.md` with build instructions.

## Use a modified axtb

1. Download and unpack the source archive for the installed package version.
2. Edit the engine files under `axtb-source/`.
3. Follow the **Python wheel with a modified axtb** section in
   `BUILD-WITH-XTB.md`. It generates the wheel's license notices, changes to
   `crates/python-xtb`, and runs `maturin build --release --offline` with the
   same seven Cargo `--config` patch overrides as the desktop rebuild. Their
   paths start with `../../axtb-source/` because the command runs from
   `crates/python-xtb`. Maturin forwards each `--config` to Cargo. Omit
   `--locked` so Cargo can record the modified local sources.
4. Install the wheel it writes under `target/wheels/`:
   `pip install --force-reinstall target/wheels/reshiki_xtb-*.whl`, run from
   the archive's top directory.

The `reshiki` and `reshiki-xtb` package versions must match. Rebuilding this
wheel keeps the package version while using the axtb changes you made.
