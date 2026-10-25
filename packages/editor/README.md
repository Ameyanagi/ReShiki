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
give the element a size. The bare imports below work with a bundler or the
[import map in the plain HTML recipe](#plain-html-page).

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

It fires `ready` and `change`, and `error` when the editor cannot start, for
example when `src` answers 404, and has `load`, `save` and `exportImage`.
`await element.ready` resolves to the mounted editor once it runs, or rejects
if it could not start. This promise also works when the `ready` event has
already fired.

### Plain HTML page

This example uses the installed npm package without a bundler. Start in an
empty folder with Bun and Python 3 installed:

```sh
bun init -y
bun add @ameyanagi/reshiki
bun -e 'import { convert } from "@ameyanagi/reshiki"; await Bun.write("caffeine.mol", await convert("CN1C=NC2=C1C(=O)N(C(=O)N2C)C", "smiles", "mol"))'
```

Save the following as `index.html` in that folder:

```html
<!doctype html>
<html lang="en">
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>My ReShiki page</title>
  <script type="importmap">
    {
      "imports": {
        "@ameyanagi/reshiki/editor": "./node_modules/@ameyanagi/reshiki/src/editor.js",
        "@ameyanagi/reshiki/element": "./node_modules/@ameyanagi/reshiki/src/element.js",
        "@ameyanagi/reshiki/embed": "./node_modules/@ameyanagi/reshiki/src/embed.js"
      }
    }
  </script>
  <reshiki-editor src="caffeine.mol" format="mol" style="height: 600px"></reshiki-editor>
  <script type="module">
    import "@ameyanagi/reshiki/element";
    const element = document.querySelector("reshiki-editor");
    await element.ready;
    console.log("Ready:", await element.save("smiles"));
  </script>
</html>
```

Serve this folder, including `node_modules`, over HTTP:

```sh
python3 -m http.server 8080 --bind 127.0.0.1
```

Open `http://127.0.0.1:8080/` in Safari or Chrome. The page shows caffeine
(C8H10N4O2), and the browser console prints `Ready:` followed by its SMILES.
Import → Name can preview `ethanol` locally. Python 3's server serves `.wasm`
as `application/wasm`; a production server must do the same.

Keep the package's `src/` and `dist/` folders beside each other when copying
them to a public directory: `src/editor.js` and `src/worker.js` load
`../dist/…`. Adjust the three import-map URLs to that directory together.
The page and package must share an origin for the naming worker; see
[Naming](#naming). Open the HTTP URL, rather than `index.html` as a local file.
The repository's `examples/serve.ts` is a development tool and is not part
of the installed package.

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

Each font added with `fonts`, `addFont` or the Japanese fallback is kept in
browser storage (below) and added again on a later visit once a caption needs
it, so the fallback is downloaded once per site rather than once per visit. The editor cannot list
or read the computer's installed fonts by itself: browsers only offer them
through Local Font Access, behind a permission prompt.

### Storage

The editor keeps what the desktop app keeps in its data folder in the
browser's IndexedDB for the page's site (database `reshiki`): appearance
settings, My templates and favorites, saved themes, your abbreviations,
added fonts and recovery drafts. They load before the editor starts, and a change in one tab
reaches the site's other tabs while they run. A tab keeps a change only if the
stored file is still the one it read, so tabs never save over each other; a
save that another tab beat, or one the browser refuses because storage is
full, is undone and the status bar says why. Messages name this storage
"Browser storage". Where the browser offers no storage, as in some private
windows, the editor starts with defaults and saving a setting, template, theme
or abbreviation says that browser storage is unavailable. Clearing the site's
data removes it all.

As in the desktop app, each drawing with unsaved changes has a recovery draft,
written within about a second of each change. If the page is reloaded, closed
or crashes, the next page of the site offers the drafts in the status bar
(**Recovery draft · Restore · Dismiss**). Drafts of the site's other open tabs
are not offered (this needs Web Locks, which current browsers have). The page
cannot write while it unloads, so a change made in the last second before a
reload can be lost.

### Content Security Policy

On a page with a Content-Security-Policy, the editor needs:

- `script-src 'wasm-unsafe-eval'` (with the source the package is served
  from), or the WebAssembly does not start and the page stays blank;
- `connect-src https://cdn.jsdelivr.net` for the Chinese, Japanese and Korean
  fallback font, unless `cjkFont` names a font on the page's own site or is
  `false`, plus the sites `fonts` and `src` load from;
- `worker-src blob:` on browsers without `scheduler.postTask`, where the
  event loop's timer runs in a small worker made from a `blob:` URL.

`style-src` can stay strict: `mount` and `<reshiki-editor>` style the editor
through the CSSOM, which `style-src` does not restrict. `src/frame.html`, the
page `embed()` loads, has an inline `<style>`, so serve it with
`style-src 'unsafe-inline'` or a page of your own.

### Naming

Name ↔ structure conversion (**Import → Name**, **Name molecule**) runs the
desktop's OPSIN Rust port and naming rules in a module Web Worker,
`src/worker.js`, which loads its own WebAssembly
(`dist/worker/reshiki_web_worker_bg.wasm`, about 2.3 MB gzipped) the first time
a name is converted, never at startup. As on the desktop, a request stops after
15 seconds and the worker's heap is limited to 96 MiB (its memory to
128 MiB), so a runaway name cannot block or crash the editor; the next
request starts a new worker.

Browsers start workers only from the page's own origin, so serve the package
from the page's site to use naming. A Content-Security-Policy whose
`script-src` allows that site needs nothing more. If it sets `worker-src` (or
`child-src`), that must allow the site too, for example
`worker-src 'self' blob:`.

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

- Naming and NMR prediction work: naming runs in its own worker (see
  [Naming](#naming)). 3D models and 3D optimization work, but pause the page
  while they compute. The assistant and printing need the desktop app and are
  hidden, Check for updates does not apply (the page updates the editor), and
  Office and LibreOffice integration are not available.
- Open and Save use the browser: Open, Insert from file and the drawing style,
  theme, template and picture choosers use the browser's file chooser, and
  files dropped on the editor open or insert as in the desktop app. Save and
  Export download a file under the drawing's name.
- Copying and pasting drawings goes through `navigator.clipboard`, which asks
  for permission to read, and carries PNG, HTML, SVG (Chrome and Edge) and
  text, not PDF, CDX or ChemDraw's own formats. Text fields copy, cut and
  paste plain text; their paste uses the browser's paste event and needs no
  permission.
- Input methods (Japanese, Chinese and Korean) type into text fields and
  captions through a hidden textarea that the editor adds to the element.
  Typing atom labels straight onto the drawing, without a text field, takes
  keys only, not composed text.
- Only Liberation Sans and a small subset of DejaVu Sans for the interface's
  key and arrow symbols are bundled, so drawing symbols they lack, such as ℃,
  ⇌ and superscript ⁺ and ⁻, need a font from `fonts`. Chinese, Japanese and
  Korean text loads a fallback (see [Fonts](#fonts)).
- Shortcuts follow the desktop app, with ⌘ as Command on a Mac, except
  three that browsers keep for their own tabs: New is Alt+N, Close tab Alt+W
  and next/previous tab Alt+] / Alt+[ (⌃N, ⌃W, ⌃] / ⌃[ on a Mac).
- Work other than [naming](#naming) runs on the page's thread, so a long
  cleanup, import, NMR prediction or 3D model pauses the page.
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
| `version()`                                        | The ReShiki release version, e.g. `0.13.0` (including any prerelease suffix). |

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
