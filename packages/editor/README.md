# @ameyanagi/reshiki

ReShiki in the browser, compiled to WebAssembly: the drawing editor itself,
and a headless library that converts structures between formats and draws
them. Both run the same Rust code as the desktop app, without a server.

The package is private and has not been published. Its name may change.

## Build

```sh
scripts/build_wasm.sh          # from the repository root
scripts/build_wasm.sh --test   # and run the tests
```

The script needs Rust (the toolchain in `rust-toolchain.toml` and its
`wasm32-unknown-unknown` target), `uv` and `bun`. It installs the matching
`wasm-bindgen` CLI and a pinned binaryen `wasm-opt` under `target/tools`. It
writes the headless library to `dist/wasm`, the editor to `dist/editor`, and
the license texts and third-party notices to `dist/licenses`.

## Editor

The editor is the desktop app's window, drawn into a page element with
WebGPU, or WebGL2 where the browser has no WebGPU. It fills the element, so
give the element a size.

```js
import { mount } from "@reshiki/editor/editor";

const editor = await mount(document.getElementById("editor"), {
  data: "CN1C=NC2=C1C(=O)N(C(=O)N2C)C",
  format: "smiles",
  onChange: () => console.log("edited"),
});
await editor.load(molfile, "mol"); // replaces the drawing in front
const mol = await editor.save("mol"); // rsk, mol, smiles, inchi, cdxml, cdx, rxn, rsmi
const svg = await editor.exportImage("svg"); // svg (text), png or pdf (bytes)
const stop = editor.onChange(() => {}); // returns a function that stops it
editor.destroy();
```

`load` takes the same formats as `convert` and rejects with the app's message
when it cannot read the data; so does `mount` for its `data`, before the
editor starts. Mount on an element in the page's body: the editor cannot
start inside a shadow root. `save` and `exportImage` return what the
headless library's `convert` would for the drawing in front.

A page runs one editor: the editor and the page share one thread and one
WebAssembly instance, and a second `mount`, or one after `destroy`, rejects.
For several editors, or to keep the page's styles away from the editor, put
each in an iframe:

```js
import { embed } from "@reshiki/editor/embed";

const editor = embed(document.getElementById("left"));
await editor.ready;
await editor.load("CCO", "smiles");
```

`embed` returns the same methods as `mount`, sent to `src/frame.html` with
`postMessage`. Serve that page with the package's `src` and `dist` folders, or
pass its URL as `frameUrl`.

The custom element wraps `mount`:

```html
<script type="module">
  import "@reshiki/editor/element";
</script>
<reshiki-editor src="caffeine.mol" format="mol" style="height: 600px"></reshiki-editor>
```

It fires `ready` and `change`, and has `load`, `save` and `exportImage`.

### Demo

```sh
scripts/build_wasm.sh                    # from the repository root
cd packages/editor && bun examples/serve.ts
```

Then open <http://localhost:8080/examples/> (one editor and the page's
buttons) or <http://localhost:8080/examples/iframe.html> (two editors in
iframes). `PORT` picks another port.

### What the browser editor leaves out

- Naming, NMR prediction and 3D say they need the desktop app. The assistant,
  updates, printing, Office and LibreOffice integration are not built.
- Open and Save use the browser: Open picks a file, Save and Export download
  one under the drawing's name. Drawing styles, themes, templates and pictures
  cannot be loaded from files yet, Insert cannot insert files, and there are
  no recovery drafts.
- Copy and paste carry text (SMILES, the drawing as text) through
  `navigator.clipboard`, which asks for permission to read. There are no
  image or ChemDraw clipboard formats.
- Only Liberation Sans is bundled, so PNG and PDF figures have no glyphs for
  CJK text or symbols it lacks, such as ℃, ⇌ and superscript ⁺ and ⁻.
- Shortcuts use Ctrl, also on macOS, where Cmd does not work yet.
- Work runs on the page's thread, so a long cleanup or import pauses the page.
- Without WebGPU, lines are drawn without antialiasing: WebGL2 loses most
  canvases with it.

## Library

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

## How the library differs from the desktop

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
