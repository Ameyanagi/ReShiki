# @ameyanagi/reshiki

ReShiki in the browser, compiled to WebAssembly: the drawing editor itself,
and a headless library that converts structures between formats and draws
them. Both run the same Rust code as the desktop app, without a server.

## Install

```sh
bun add @ameyanagi/reshiki
npm install @ameyanagi/reshiki
pnpm add @ameyanagi/reshiki
```

Each ReShiki release publishes the package with the app's version, so
`@ameyanagi/reshiki@0.13.0` is the editor of ReShiki 0.13.0. Prereleases are
under the `next` tag (`bun add @ameyanagi/reshiki@next`). The package carries
the license texts and third-party notices in `dist/licenses`.

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
import { mount } from "@ameyanagi/reshiki/editor";

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

Leaving the page while any tab has unsaved changes (reload, closing the
browser tab, following a link) asks first with the browser's "Leave site?"
prompt. Closing an unsaved tab inside the editor asks Save, Don't Save or
Cancel. The page can follow the same state:

```js
const editor = await mount(element, {
  title: true, // document.title follows the drawing: "Untitled • — ReShiki"
  onDirtyChange: (dirty) => console.log(dirty ? "unsaved" : "saved"),
  onTitleChange: (title) => console.log(title),
});
editor.isDirty(); // whether any tab has unsaved changes
const stopDirty = editor.onDirtyChange((dirty) => {}); // returns a function that stops it
const stopTitle = editor.onTitleChange((title) => {});
```

The title is `Name — ReShiki`, with a `•` after the name while the tab in
front has unsaved changes, as on the desktop. Without `title: true` the page
keeps its own title, and `onTitleChange` hands it the editor's.

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
import { embed } from "@ameyanagi/reshiki/embed";

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
  import "@ameyanagi/reshiki/element";
</script>
<reshiki-editor src="caffeine.mol" format="mol" style="height: 600px"></reshiki-editor>
```

It fires `ready` and `change`, and has `load`, `save` and `exportImage`.

### Fonts

The editor can only draw with fonts loaded into it as bytes: it bundles
Liberation Sans, which has Arial's letter widths and answers to Arial, so
drawings lay out as on the desktop. Text that font lacks falls back to the
fonts below.

- **Chinese, Japanese and Korean text.** The first time a caption, a tab's
  file name or the status line has such characters that no
  loaded font draws, the editor loads a fallback once for that language. If
  the page already has Local Font Access permission (Chrome and Edge), it uses
  the computer's own font, such as Hiragino Sans, Apple SD Gothic Neo or
  PingFang SC, without a prompt. Otherwise it downloads a
  [Noto Sans](https://github.com/notofonts/noto-cjk) Regular subset (SIL Open
  Font License 1.1) from jsDelivr: JP (4.5 MB) for kana and kanji, KR
  (4.6 MB) for Hangul, and SC (8.3 MB) for Bopomofo and for ideographs the
  Japanese font lacks. Pages without such text download nothing. The fonts
  are not bundled in this package.
- **The page's own fonts.** `fonts` takes TrueType or OpenType files (or
  collections) as URLs or bytes. They load before the editor starts, and
  text whose font lacks a glyph falls back to them. URLs on another origin
  need CORS.

```js
await mount(element, {
  fonts: ["/fonts/MyLabels.ttf"],
  cjkFont: "/fonts/NotoSansCJK-Regular.otc", // one self-hosted fallback for every language, or false for none
  onError: (error) => console.warn(error.message), // a font that could not load
});
```

`embed()` takes the same options, with fonts as URLs. The custom element
takes `fonts` (URLs separated by spaces) and `cjk-font` (a URL, or `false`)
attributes and fires `error` for a font that could not load. Fonts added this
way also reach `save` and `exportImage` figures.

### Demo

```sh
scripts/build_wasm.sh                    # from the repository root
cd packages/editor && bun examples/serve.ts
```

Then open <http://localhost:8080/>, which is the editor alone, filling the
window. <http://localhost:8080/examples/api.html> calls the JavaScript API from
the page's own buttons, and <http://localhost:8080/examples/iframe.html> shows
two editors in iframes. `PORT` picks another port.

The same editor-only page runs at <https://reshiki.com/demo/>, packaged by
`scripts/package_web_demo.sh` with a dismissible note that it is a browser
preview, not the full app. The list below is what it leaves out; point people
to the [desktop app](https://reshiki.com/guide/install/) whenever they can use
it.

### What the browser editor leaves out

- Naming, NMR prediction and 3D say they need the desktop app. The assistant,
  updates, printing, Office and LibreOffice integration are not built.
- Open and Save use the browser: Open picks a file, Save and Export download
  one under the drawing's name. Drawing styles, themes, templates and pictures
  cannot be loaded from files yet, Insert cannot insert files, and there are
  no recovery drafts.
- Copying and pasting drawings goes through `navigator.clipboard`, which asks
  for permission to read, and carries PNG, HTML, SVG (Chrome and Edge) and
  text, not PDF, CDX or ChemDraw's own formats. Text fields copy, cut and
  paste plain text; their paste uses the browser's paste event and needs no
  permission.
- Only Liberation Sans and a small subset of DejaVu Sans for the interface's
  key and arrow symbols are bundled, so drawing symbols they lack, such as ℃,
  ⇌ and superscript ⁺ and ⁻, need a font from `fonts`. Chinese, Japanese and
  Korean text loads a fallback (see [Fonts](#fonts)).
- Shortcuts follow the desktop app, with ⌘ as Command on a Mac, except
  three that browsers keep for their own tabs: New is Alt+N, Close tab Alt+W
  and next/previous tab Alt+] / Alt+[ (⌃N, ⌃W, ⌃] / ⌃[ on a Mac).
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

| Function                                           | Result                                                                        |
| -------------------------------------------------- | ----------------------------------------------------------------------------- |
| `convert(data, from, to)`                          | The structure or drawing in another format.                                   |
| `renderSvg(data, format)`                          | The editor's SVG preview, with a transparent surround.                        |
| `renderPng(data, format, { maxWidth, maxHeight })` | The editor's PNG preview on white, scaled to fit (1600 × 1000 by default).    |
| `smiles(data, format)`                             | The canonical SMILES.                                                         |
| `inchi(data, format)`                              | The standard InChI.                                                           |
| `addFont(font)`                                    | Adds a font (a URL or bytes) that figures fall back to, such as for Japanese. |
| `version()`                                        | The library version.                                                          |

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

Pull request CI runs these tests, after Clippy for wasm32, in the **Browser
build (wasm32)** job of the Checks workflow; see
[Development](../../docs/development.md#ci).
