# Native structure highlight fixtures

`native-prime.cdxml` and `native-prime.cdx` were saved through the actual
ChemDraw Prime 26.0.0.6599 macOS application on 2026-10-03. The application
opened and visibly rendered a small scientific graph fixture, then its native
Save As dialogs wrote these two files. They are not ReShiki codec output.

The input contains only five atoms and three bonds: a three-atom carboxyl
subgraph with a cyan OH label, and a two-carbon single bond with yellow atom
and bond highlights. The scientific graph, relative coordinates, and color
indices were extracted from the installed vendor's `Shortcuts Cheat Sheet for
macOS.cdxml` sample; coordinates were translated, color indices remapped,
and atom text labels independently reconstructed from element/hydrogen data.
The omitted neighbors become implicit hydrogens. No sample captions,
illustrations, or page layout are included. The full proprietary sample is
not redistributed here.

The bundled source's SHA-256 is
`7cea18cf7cd93640df20d37f170b85bc6aed9986c340bf556c0a7b23a720e9ed`.
The small extracted input's SHA-256 is
`cb8f11288ee1ed7dc3749cc0acb8ec86d99cd4fccf1da602c82410cd54eb089a`.

| Native output | Bytes | SHA-256 |
| --- | ---: | --- |
| `native-prime.cdxml` | 3619 | `e79bb62077dd7847c96e3c095e79d52aecad95e68fc7f90df3afa8fcc21796b3` |
| `native-prime.cdx` | 1629 | `74cbe8aea7e614ef94e73c1ab1211959c34708150eaea3f0956ac69989ddc757` |

Native CDXML uses the `highlightColor` attribute on `n` and `b` objects.
The matching CDX property is `0x0308`, a two-byte little-endian color-table
index. This was also checked against all 13 highlighted atoms and 12
highlighted bonds in a private native save of the original sample. Ordinary
foreground color is a separate property.

These fixtures establish native import/display/save preservation. ChemDraw
Prime has no highlight authoring toolbar; Professional toolbar authoring is
outside this fixture's evidence. The contracted-label tests use an
independently authored OMe graph and the documented fill-only-missing
propagation rule; they do not claim native toolbar authoring.
