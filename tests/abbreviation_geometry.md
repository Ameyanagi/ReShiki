# Abbreviation geometry references

The 29 preset fragments use coordinates captured from RDKit 2026.03.6.
`abbreviation_replacement` compares each platform's table with freshly generated
coordinates before checking complete replacement documents. These tables preserve
the existing layout until the general Rust layout implementation replaces it.

Windows ARM runs the pinned x64 Python/RDKit worker under emulation. Eight presets
have different floating-point coordinates from a native Windows x64 run. The ARM
table therefore follows the host architecture, even though the worker is x64.
Its initial capture is the unmodified `--geometry` output from
[Windows ARM CI job 106599080649](https://github.com/Ameyanagi/ReShiki/actions/runs/35681450231/job/106599080649)
at commit `7eaa9f97f03ccf70a1bfb9874d4e9223b14075aa`.

To regenerate on the matching Windows host:

```sh
uv run --locked python tests/abbreviation_replacement_reference.py --write-geometry --target-arch aarch64
```

Use `x86_64` for a native Windows x64 host. On macOS and Linux, omit
`--target-arch` to use the Python process architecture. Regenerate only with the
pinned reference environment; never substitute another host's coordinates or
relax the exact document comparisons to make a test pass.
