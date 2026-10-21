# @ameyanagi/reshiki

ReShiki's chemistry in the browser, compiled to WebAssembly. It converts
structures between formats and draws them with the same Rust code as the
desktop app, without a server. The interactive editor is not included yet.

The package is private and has not been published. Its name may change.

## Build

```sh
scripts/build_wasm.sh          # from the repository root
scripts/build_wasm.sh --test   # and run the tests
```

The script needs Rust (the toolchain in `rust-toolchain.toml` and its
`wasm32-unknown-unknown` target), `uv` and `bun`. It installs the matching
`wasm-bindgen` CLI and a pinned binaryen `wasm-opt` under `target/tools`. It
writes the module to `dist/wasm` and the license texts and third-party notices
to `dist/licenses`.

## Use

```js
import { convert, renderPng, renderSvg, smiles, inchi } from "@ameyanagi/reshiki";

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
import { init } from "@ameyanagi/reshiki";
await init(new URL("/assets/reshiki_web_bg.wasm", location.href));
```

| Function                                           | Result                                                                     |
| -------------------------------------------------- | -------------------------------------------------------------------------- |
| `convert(data, from, to)`                          | The structure or drawing in another format.                                |
| `renderSvg(data, format)`                          | The editor's SVG preview, with a transparent surround.                     |
| `renderPng(data, format, { maxWidth, maxHeight })` | The editor's PNG preview on white, scaled to fit (1600 × 1000 by default). |
| `smiles(data, format)`                             | The canonical SMILES.                                                      |
| `inchi(data, format)`                              | The standard InChI.                                                        |
| `version()`                                        | The library version.                                                       |

`data` is a string or a `Uint8Array`; `cdx` input must be bytes.

- Input formats: `rsk` (a ReShiki drawing), `mol`, `smiles`, `inchi`, `cdxml`,
  `cdx`, `rxn`, `rsmi` (reaction SMILES) and `auto`, which tells the text
  formats apart.
- Output formats: `rsk`, `mol`, `smiles`, `inchi`, `cdxml`, `rxn`, `rsmi` and
  `svg` (strings), and `cdx`, `png` and `pdf` (bytes). `svg`, `png` and `pdf`
  are figures as the desktop's Export makes them.

Failures reject with an `Error` that carries the app's message.

## How it differs from the desktop

- SMILES, MOL, InChI and CDXML output matches `reshiki --cli convert` on macOS.
  The tests check this when `RESHIKI_CLI` names a native `reshiki` binary.
- Labels are measured and drawn with the bundled Liberation Sans, which has
  Arial's metrics. Coordinates near labels can differ from the desktop's by
  under one drawing unit. SVG text names Arial, so a browser draws it with the
  viewer's Arial or a substitute.
- Everything runs on the calling thread. Large drawings block the page until
  they finish, and InChI and 3D work have no time limit in the browser. Run the
  module in a Web Worker to keep a page responsive.

## Test

```sh
cd packages/editor
bun test                                          # needs a built dist/
RESHIKI_CLI=../../target/debug/reshiki bun test   # also compares with the CLI
```
