# xTB integration fixtures

These geometries and reference data are ReShiki test fixtures, licensed under
`MIT OR Apache-2.0`, like the surrounding ReShiki code.

- `water.xyz`: water geometry written by hand.
- `caffeine.xyz`: caffeine generated with RDKit ETKDG, random seed `2519`,
  followed by MMFF94 minimization.
- `ferrocene.xyz`: idealized eclipsed D5h ferrocene geometry written by hand.

XYZ coordinates are in Ångström. All three calculations use neutral molecules,
the default unpaired-electron count, GFN2-xTB, no solvent, and one thread.

`reference.json` preserves the energy and every gradient component from the
independent axtb CLI output. The reference runs used axtb `0.2.0`, revision
`06e29058416b26b69db9715728f637b43558adff` (the fork's `reshiki` branch; upstream
`65d0f64a9a8211f73b0738ff908b7a414975f50b` gives the same bits), on macOS arm64,
with one thread, built from the fork's workspace with its `gemm` patches. The
commands below reproduce the reference runs.

```sh
axtb water.xyz --grad
axtb caffeine.xyz --grad
axtb ferrocene.xyz --grad
```

Energies are in Eh and gradients in Eh/bohr. The source CLI reports numerics
identity `gfn2:1` and the `faer` eigensolver. The root integration suite compares
both the worker Session API and `reshiki --cli calc --energy --gradient` against
these values, with absolute tolerances of `1e-10` Eh for energy and `1e-9`
Eh/bohr for each gradient component.
