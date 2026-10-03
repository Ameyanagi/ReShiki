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
| `native-prime.svg` | 2134 | `1bbbd27dfdf36174b68626302b201c995f7846bde4d4086a9b603c780fe90b0d` |

The SVG was saved through Prime from the same small input. It is an independent
geometry reference: at Arial 10 pt its unlabelled atom/bond capsule radius is
approximately 3.092 pt. The OH halo follows the text ink bounds with elliptical
caps of radius 2.624805 pt horizontally and 5.2496 pt vertically. Renderer tests
compare the native capsule path directly; text halo extents use the actual
installed font's ink bounds so other platforms retain legible padding.

Native CDXML uses the `highlightColor` attribute on `n` and `b` objects.
The matching CDX property is `0x0308`, a two-byte little-endian color-table
index. This was also checked against all 13 highlighted atoms and 12
highlighted bonds in a private native save of the original sample. Ordinary
foreground color is a separate property.

These fixtures establish native import/display/save preservation. ChemDraw
Prime has no highlight authoring toolbar; Professional toolbar authoring is
outside this fixture's evidence.

## Native clipboard and contracted labels

An independently authored OMe graph was opened in Prime, copied, pasted into
a new native document, and saved as `native-contracted.cdxml`. Prime's
**Structure > Expand Label** command then produced `native-expanded.cdxml`.
These are actual native saves from 2026-10-03, not simulated clipboard output.
The visible label had cyan paint, the internal oxygen had red paint, and the
internal bond had yellow paint. Expansion kept that red/yellow paint, filled
the previously clear internal carbon with cyan, and left the external carbon
and bond clear.

| Native output | Bytes | SHA-256 |
| --- | ---: | --- |
| `native-contracted.cdxml` | 3654 | `98619747e3c7e4a8872e233b8fb154994c22280e7714da2dba56d3a4ed0571b4` |
| `native-expanded.cdxml` | 3244 | `1df0d101b93df02fb09b249d6dfd405d7de30aae447ddfbf5c69bdf6df151e74` |

The original short-decimal fixture encoded green `230/255` as `0.901960784`
and `198/255` as `0.776470588`. Prime truncated those values to green 229 and
197, respectively. The regression fixtures intentionally preserve the actual
native result: cyan `[129,229,255]`, yellow `[255,197,0]`, and red `[223,71,62]`.
ReShiki's writer now emits a decimal immediately above each interior channel
boundary, with a normalized bias of at most `1e-8`; black/white endpoints and
the exact 16-bit CDX channels stay unchanged. Tests cover all 256 channel
values under truncating and rounding receivers.

An actual Prime 26.0.0.6599 open/display/Save As check then verified a prepared
256-node grayscale grid using that same decimal encoding. Each saved node ID
was matched to its original channel: all 256 saved colors reconstructed to the
intended 8-bit RGB value, with no missing nodes or mismatches. The source file
hash remained unchanged. [The compact native receipt](color-precision-provenance.json)
records input/output hashes and every source/native palette component. Native
components use ChemDraw's four-decimal output precision, so this comparison
recovers each 8-bit channel by rounding `component * 255`.

Native paste and Save completed asynchronously. An immediate read briefly
showed an empty page; the completed file and visible pasted structure were
subsequently verified. That timing observation is not evidence of a malformed
input or failed clipboard transfer.

## Independent contracted and internal text

`native-independent-label-ink.cdxml` is an actual Prime 26.0.0.6599 native
Save As of the production ReShiki OMe test drawing. Its contracted label has
RGB `[124,124,124]` ink on a black highlight; the oxygen inside it has black
ink on a white highlight. Both use Helvetica 10 pt, and the document bond
length is 28 pt. The internal carbon and bond retain their cyan and yellow
highlights. [The provenance receipt](independent-label-ink-provenance.json)
records the source/native hashes and full relevant style values.

The 3,341-byte native fixture has SHA-256
`d5e5cbde4d4353b2fa23e1444147486c6ad27cafeba8043718dde1d460fd3d4c`.
It establishes native open/save preservation of both text presentations.
A subsequent native copy/paste into a matching Helvetica 10 pt / 28 pt bond
document also retained the gray OMe label, black internal oxygen, and all
internal highlights. The completed native paste was 3,516 bytes, SHA-256
`a17ffe06dfa344a6919a4fac3b1678b032ca6802f7e008dd10e012f224b12c6b`.

Prime's subsequent **Structure > Expand Label** command changed the internal
oxygen, carbon, and bond foregrounds to the wrapper's RGB `[124,124,124]`.
Their white, cyan, and yellow highlights remained unchanged; the external
bond stayed black. The completed native expansion was 3,175 bytes, SHA-256
`511be84c703e049698d6675d2fa7c462febcd7363b47c54957de97ff9768dc02`.
The provenance receipt records this native command's foreground inheritance
separately from the successful matched-style clipboard transfer.

ReShiki's input already explicitly writes black as `color="3"` on the internal
oxygen text and bond. Prime removes these redundant default-black attributes
when saving. Reserved black index `0` has not been verified to prevent the
native command's foreground inheritance. ReShiki keeps the original internal
styles during its own expansion. The regression imports the native contracted
file, saves/reopens ReShiki data, exports/imports CDXML and CDX, then expands
the group in ReShiki while checking both presentations independently.
