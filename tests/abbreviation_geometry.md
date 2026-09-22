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

Windows ARM's x64 reference also uses the CRT's non-FMA transcendental functions.
The ARM Rust CRT uses a different sine algorithm: at one TBS attachment the last
f64 bit puts a drawing coordinate on opposite sides of an f32 rounding midpoint.
[The native CI trace](https://github.com/Ameyanagi/ReShiki/actions/runs/35684363028/job/106607926464)
identifies sine as the only differing rotation value. Disabling FMA3 in a separate
physical Windows x64 Python process reproduces both all 29 ARM template coordinates
and the exact failing TBS result. This is a documented
[CRT algorithm difference](https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/get-fma3-enable-set-fma3-enable).

Windows ARM therefore uses a bounded safe Rust adaptation of AMD win-libm's
non-FMA sine/cosine paths for attachment rotations. Other targets retain their
native math. This does not change the reference worker or system CRT settings.
The adaptation rejects nonfinite values and angles outside ±2π; two finite
`atan2` values cannot produce a rotation outside that range.

`abbreviation_trigonometry_reference.py` captures 42,567 independent native
sine/cosine pairs, including quadrant boundaries, signed zeros, subnormals,
dense and random angles. The checked-in binary contains big-endian f64 bit
triples (angle, sine, cosine); its adjacent JSON records the native environment.
The Rust unit test checks every bit on every platform, while
`abbreviation_replacement` still checks complete documents against the unchanged
original worker on the matching host.

To regenerate the math corpus with x64 CPython on Windows:

```sh
uv run --locked python tests/abbreviation_trigonometry_reference.py tests/fixtures/abbreviation-trigonometry-windows.bin
```

The capture temporarily disables FMA3 only inside that Python process and restores
its previous flag before exit. No Rust runtime component calls that API.
