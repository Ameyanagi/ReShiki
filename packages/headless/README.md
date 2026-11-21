# @ameyanagi/reshiki-headless

ReShiki without the editor, compiled to WebAssembly: a library that converts
structures between formats and draws them. It runs the same Rust code as the
desktop app, without a server, in a browser, Bun, Node or Deno.

For the drawing editor in a page, use
[`@ameyanagi/reshiki`](../editor/README.md).

## Install

```sh
bun add @ameyanagi/reshiki-headless
npm install @ameyanagi/reshiki-headless
pnpm add @ameyanagi/reshiki-headless
```

Each ReShiki release publishes the package with the app's version, so
the package's version is the ReShiki release it was built from.
Prereleases are under the `next` tag
(`bun add @ameyanagi/reshiki-headless@next`). The package carries the license
texts and third-party notices in `dist/licenses`.

## Moving from 0.15

Up to ReShiki 0.15.0 this library was the main entry of `@ameyanagi/reshiki`,
so every install downloaded the editor, its worker and the library. Later
releases publish two packages, each with only its own WebAssembly. The
functions are unchanged; only the package name is new:

| ReShiki 0.15.0 and earlier                     | Later releases                                          |
| ---------------------------------------------- | ------------------------------------------------------- |
| `import { convert } from "@ameyanagi/reshiki"` | `import { convert } from "@ameyanagi/reshiki-headless"` |
| `@ameyanagi/reshiki/reshiki_web_bg.wasm`       | `@ameyanagi/reshiki-headless/reshiki_web_bg.wasm`       |

`@ameyanagi/reshiki/editor`, `/element`, `/embed`, `/frame.html` and
`/reshiki_bg.wasm` stay where they are.

## Use

```js
import { convert, renderPng, renderSvg, smiles, inchi } from "@ameyanagi/reshiki-headless";

const mol = await convert("CN1C=NC2=C1C(=O)N(C(=O)N2C)C", "smiles", "mol");
const svg = await renderSvg(mol, "mol");
const png = await renderPng(mol, "mol", { maxWidth: 800, maxHeight: 600 });
const id = await inchi(mol, "mol");
```

Each function loads the module on its first call, from `reshiki_web_bg.wasm`
next to the package's JavaScript. Bundlers such as Vite and webpack follow
that `new URL(..., import.meta.url)` reference. To load it from somewhere else,
call `init` first with a URL, a `Response`, the bytes or a compiled
`WebAssembly.Module`:

```js
import { init } from "@ameyanagi/reshiki-headless";
await init(new URL("/assets/reshiki_web_bg.wasm", location.href));
```

| Function                                           | Result                                                                        |
| -------------------------------------------------- | ----------------------------------------------------------------------------- |
| `read(data, format)`                               | `{ drawing, warnings }`: the native drawing (`rsk`) and the import warnings.  |
| `convert(data, from, to, { svgText })`             | The structure or drawing in another format.                                   |
| `renderSvg(data, format, { svgText })`             | The editor's SVG preview, with a transparent surround.                        |
| `renderPng(data, format, { maxWidth, maxHeight })` | The editor's PNG preview on white, scaled to fit (1600 × 1000 by default).    |
| `smiles(data, format)`                             | The canonical SMILES.                                                         |
| `inchi(data, format)`                              | The standard InChI.                                                           |
| `predictNmr(data, format, nucleus)`                | Offline `"1H"` or `"13C"` shifts, reference statistics and data credit.       |
| `addFont(font)`                                    | Adds a font (a URL or bytes) that figures fall back to, such as for Japanese. |
| `version()`                                        | The ReShiki release version, e.g. `0.16.0` (including any prerelease suffix). |

`data` is a string or a `Uint8Array`; `cdx` input must be bytes.

- Input formats: `rsk` (a ReShiki drawing), `mol`, `smiles`, `inchi`, `cdxml`,
  `cdx`, `rxn`, `rsmi` (reaction SMILES) and `auto`, which tells the text
  formats apart.
- Output formats: `rsk`, `mol`, `smiles`, `inchi`, `cdxml`, `rxn`, `rsmi` and
  `svg` (strings), and `cdx`, `png` and `pdf` (bytes). `svg`, `png` and `pdf`
  are figures as the desktop's Export makes them.
- `reshiki` is accepted wherever `rsk` is: it is the name the MCP tools and
  `reshiki --cli` use for the same format.

SVG from `convert` and `renderSvg` draws text as outlines, so it looks the
same in every viewer. Pass `{ svgText: "live" }` to keep `<text>` elements,
which a viewer without the drawing's font draws with a substitute. This is the
`svg_text` option of the MCP tools, the command line and the Python package,
with the same default.

Failures reject with an `Error` that carries the app's message and a `code`,
the [error code](https://reshiki.com/developer/agent-api/#error-codes) the MCP
tools and the command line report for the same failure:
`invalid_arguments` for a format, option or nucleus that is not accepted, and `failed`
when the structure cannot be read, drawn or written.

```js
try {
  await convert("C(", "smiles", "mol");
} catch (error) {
  console.log(error.code, error.message); // failed Invalid SMILES at byte 2
}
```

Some inputs import with a change or an assumption, such as a SMILES bond of any
order (`~`), which is drawn as a single bond. The app shows
a warning for these; `convert`, `renderSvg`, `renderPng`, `smiles`, `inchi` and `predictNmr`
return only their result. To see the warnings, `read` the input first and
continue from its drawing:

```js
import { convert, read } from "@ameyanagi/reshiki-headless";

const { drawing, warnings } = await read("C~C", "smiles");
for (const warning of warnings) console.warn(warning);
const mol = await convert(drawing, "rsk", "mol");
```

## Predict NMR

```js
import { predictNmr } from "@ameyanagi/reshiki-headless";

const prediction = await predictNmr("CCOC(C)=O", "smiles", "13C");
for (const row of prediction.rows) console.log(row.atom, row.ppm);
console.log(prediction.credit); // cite this with published values
```

Use `"1H"` for proton shifts or `"13C"` for carbon shifts. The drawing must
contain exactly one molecule. Prediction runs offline with the same reference
index and result as `reshiki --cli predict-nmr`, without its `revision` field.
The result contains `nucleus`, `solvent`, `rows`, `method`, `data_version`,
`conditions`, `limitations`, `credit` and `data_access`.

Each row contains `atom`, `nucleus`, `ppm`, `shell`, `references`, `sd`, `min`,
`max`, `broad`, `hydrogen_count`, `hydrogens` and `note`. Atom IDs, including
the attached explicit H IDs in `hydrogens`, are decimal strings. A proton row
groups the H attached to one heavy atom; it is not an assigned stereospecific
peak. Sites without reference data retain their row, null statistics and a
note. `hydrogen_count` is null for carbon rows.

The reference data comes from nmrshiftdb2 under the
[nmrshiftdb2 Database License](https://github.com/Ameyanagi/ReShiki/blob/main/licenses/nmrshiftdb2/DATABASE-LICENSE.txt),
which the package includes. Cite the returned `credit` with published values.
`data_access` links to the derivative index and its transformation method.
Read `conditions` and `limitations` with the values: these are connectivity
based chemical shifts in CDCl3, with no coupling or multiplet prediction.

An unsupported nucleus rejects with `invalid_arguments`. More than one
molecule, unsupported chemistry or no sites for the requested nucleus reject
with `failed`. A molecule whose sites all lack reference data still returns
its rows.

## How the library differs from the desktop

- SMILES, MOL, InChI and CDXML output matches `reshiki --cli convert` on macOS.
  The tests check this when `RESHIKI_CLI` names a native `reshiki` binary.
- Labels are measured and drawn with the bundled Liberation Sans, which has
  Arial's metrics. Coordinates near labels can differ from the desktop's by
  under one drawing unit. Outlined SVG text is drawn with Liberation Sans
  glyphs; live SVG text names Arial, so a browser draws it with the viewer's
  Arial or a substitute.
- Everything runs on the calling thread. Large drawings block the page until
  they finish, and InChI and 3D work have no time limit in the browser. Run the
  module in a Web Worker to keep a page responsive.

## Build and test

```sh
scripts/build_wasm.sh          # from the repository root
cd packages/headless
bun test                                          # needs a built dist/
RESHIKI_CLI=../../target/debug/reshiki bun test   # also compares with the CLI
```

The script needs Rust (the toolchain in `rust-toolchain.toml` and its
`wasm32-unknown-unknown` target), `uv` and `bun`, and writes the library to
`dist/wasm`. Pull request CI runs these tests, after Clippy for wasm32, in the
**Browser build (wasm32)** job of the Checks workflow; see
[Development](../../docs/development.md#ci).
