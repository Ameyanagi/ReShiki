# Abbreviation geometry references

## Current source: the catalog and native depiction

Every abbreviation is one entry in
[`catalog.json`](../crates/model/src/chemistry/abbreviations/catalog.json). The
template coordinates are not stored. On first use, each group's SMILES is laid
out with `depict::compute`, the Rust port of RDKit 2026.03.6 `Compute2DCoords`
that SMILES import also uses. Adding a group therefore needs no Python, RDKit or
per-platform capture.

The depiction port reproduces the pinned RDKit wheel's floating-point behaviour
on each supported host (see [the depiction pipeline](depict_pipeline.md)).
For the original 62 groups it gives the same coordinates as the former captured
tables, bit for bit on Apple Silicon. Existing documents and replacements
therefore keep their layouts.

Two checks keep it that way:

- `own_depiction_reproduces_the_frozen_rdkit_captures`
  (`crates/model/src/chemistry/abbreviations/replacement/tests.rs`) compares the
  catalog layouts with the frozen captures in
  `replacement/rdkit-geometry-*.json`. The `Rust geometry` CI job runs it on all
  six native targets. Apple Silicon must match exactly. Other hosts allow 1e-9
  bond units, because Intel macOS compares against the Apple capture it used
  before.
- With `--features rdkit-reference`, `abbreviation_replacement` compares the
  catalog layouts with a fresh RDKit `Compute2DCoords` of every catalog SMILES,
  then checks complete replacement documents against the original worker.
  `abbreviation_detection` likewise compares the matching query that ReShiki
  derives from each SMILES with RDKit's `ParseAbbreviations` query.

The frozen captures are never regenerated. They record the layouts that
documents saved before the catalog relied on.

## History of the frozen captures

The 62 frozen templates were captured from RDKit 2026.03.6 by the retired
`Abbreviation geometry capture` workflow. TBDPS and OTBDPS were captured on all
five native reference ABIs in
[capture run 36483117507](https://github.com/Ameyanagi/ReShiki/actions/runs/36483117507)
([provenance](../tests/fixtures/tbdps-geometry-provenance.json)). The 31
carbohydrate and protecting-group presets (OBn through OC(NH)CCl3) were captured
in [capture run 39077956792](https://github.com/Ameyanagi/ReShiki/actions/runs/39077956792)
([provenance](../tests/fixtures/protecting-group-geometry-provenance.json)).
The Windows artifacts use CRLF; committed files normalize line endings to LF.

Windows ARM ran the pinned x64 Python/RDKit worker under emulation, so its
capture follows that emulated worker. Its first capture is the unmodified
`--geometry` output from
[Windows ARM CI job 106599080649](https://github.com/Ameyanagi/ReShiki/actions/runs/35681450231/job/106599080649).

## Attachment rotation on Windows ARM

Replacement rotates the template onto the attachment bond. Windows ARM's x64
reference also uses the CRT's non-FMA transcendental functions. The ARM Rust CRT
uses a different sine algorithm: at one TBS attachment the last f64 bit puts a
drawing coordinate on opposite sides of an f32 rounding midpoint.
[The native CI trace](https://github.com/Ameyanagi/ReShiki/actions/runs/35684363028/job/106607926464)
identifies sine as the only differing rotation value. This is a documented
[CRT algorithm difference](https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/get-fma3-enable-set-fma3-enable).

Windows ARM therefore uses a bounded safe Rust adaptation of AMD win-libm's
non-FMA sine/cosine paths for attachment rotations. Other targets retain their
native math. The adaptation rejects nonfinite values and angles outside ±2π; two
finite `atan2` values cannot produce a rotation outside that range.

`abbreviation_trigonometry_reference.py` captures 42,567 independent native
sine/cosine pairs, including quadrant boundaries, signed zeros, subnormals,
dense and random angles. The checked-in binary contains big-endian f64 bit
triples (angle, sine, cosine); its adjacent JSON records the native environment.
The Rust unit test checks every bit on every platform.

To regenerate the math corpus with x64 CPython on Windows:

```sh
uv run --locked python reference/abbreviation_trigonometry_reference.py tests/fixtures/abbreviation-trigonometry-windows.bin
```

The capture temporarily disables FMA3 only inside that Python process and restores
its previous flag before exit. No Rust runtime component calls that API.
