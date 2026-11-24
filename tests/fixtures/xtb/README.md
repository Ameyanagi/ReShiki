# xTB integration fixtures

These geometries and reference data are ReShiki test fixtures, licensed under
`MIT OR Apache-2.0`, like the surrounding ReShiki code.

- `water.xyz`: water geometry written by hand.
- `caffeine.xyz`: caffeine generated with RDKit ETKDG, random seed `2519`,
  followed by MMFF94 minimization.
- `ferrocene.xyz`: idealized eclipsed D5h ferrocene geometry written by hand.

XYZ coordinates are in Ångström. All three calculations use neutral molecules,
the default unpaired-electron count, GFN2-xTB, no solvent, and one thread.

`reference.json` contains two top-level reference sets, `axtb` and `xtb-6.8`,
each keyed by molecule name (`water`, `caffeine`, `ferrocene`). Each molecule
has `energy`, `gradient`, and `homo_lumo_gap` fields copied from the independent
axtb CLI output. Energies and HOMO–LUMO gaps are in Eh. Every gradient component
is in Eh/bohr. Shared engine, method, numerics, and unit metadata remain at the
top level.

The reference runs used axtb `0.2.0`, revision
`06e29058416b26b69db9715728f637b43558adff` (the fork's `reshiki` branch), on
macOS arm64, with one thread, built from the fork's workspace with its `gemm`
patches. For the original `axtb` set, upstream revision
`65d0f64a9a8211f73b0738ff908b7a414975f50b` gives the same bits. The source CLI
reports numerics identity `gfn2:1` and the `faer` eigensolver for both sets.

The `axtb` set preserves the original energy and every gradient component,
with gaps added from the same CLI reference outputs. The commands below use
the default axtb convention.

```sh
axtb water.xyz --grad
axtb caffeine.xyz --grad
axtb ferrocene.xyz --grad
```

The `xtb-6.8` set uses xtb 6.8 compatible conventions. ReShiki's protocol name
`xtb-6.8` maps to the axtb driver's `xtb` convention, which the source CLI
reports as `"conventions": "xtb"`. The commands below reproduce the references.

```sh
axtb water.xyz --grad --conventions xtb
axtb caffeine.xyz --grad --conventions xtb
axtb ferrocene.xyz --grad --conventions xtb
```

Each convention uses its own XYZ Ångström-to-bohr conversion, as well as its
driver settings. The root integration suite compares both the worker Session
API and `reshiki --cli calc --energy --gradient --conventions axtb|xtb-6.8`
against the corresponding set. The tolerances are `1e-10` Eh for energy,
`1e-7` Eh/bohr for each gradient component, and `1e-9` eV for the gap.
The fork's cross-platform validation measured differences up to `7e-8`
Eh/bohr for gradient components and `4e-12` Eh for energies. The tests use the
CODATA 2022 factor `27.211386245981` eV/Eh to convert the fixture's
`homo_lumo_gap` value.
