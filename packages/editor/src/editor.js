// The interactive editor: the desktop app built for the browser, mounted into
// a page element. The page and the editor share one thread and one WebAssembly
// instance, so a page holds one editor; embed.js puts more in iframes.
import load, * as wasm from "../dist/editor/reshiki.js";
import { loadFonts } from "./fonts.js";

const BINARY = new Set(["png", "pdf", "cdx"]);
const FIGURES = new Set(["svg", "png", "pdf"]);
const encoder = new TextEncoder();
const decoder = new TextDecoder();
let loading;

export function initEditor(module) {
  loading ??= load(module === undefined ? undefined : { module_or_path: module });
  return loading.then(() => undefined);
}

const bytes = (data) => (typeof data === "string" ? encoder.encode(data) : data);

export async function mount(element, options = {}) {
  await initEditor(options.module);
  // The drawing is read before the editor starts, so data that cannot be read
  // rejects here and the page can mount again.
  const data = options.data === undefined ? undefined : bytes(options.data);
  const onMissingFont = await loadFonts(wasm, options);
  await wasm.mount(element, data, options.format ?? "auto", onMissingFont, options.title === true);
  // Each event's callbacks; the module holds one function per event.
  const listeners = { change: [], dirty: [], title: [] };
  const subscribe = (event, callback) => {
    listeners[event] = [...listeners[event], callback];
    return () => {
      listeners[event] = listeners[event].filter((listener) => listener !== callback);
    };
  };
  const emit = (event) => (value) => listeners[event].forEach((listener) => listener(value));
  const editor = {
    async load(data, format = "auto") {
      await wasm.load(bytes(data), format);
    },
    async save(format = "rsk") {
      const output = await wasm.save(format);
      return BINARY.has(format) ? output : decoder.decode(output);
    },
    exportImage(format = "svg") {
      if (!FIGURES.has(format)) {
        return Promise.reject(new Error(`Unknown image format ${format}`));
      }
      return editor.save(format);
    },
    onChange(callback) {
      return subscribe("change", () => callback());
    },
    isDirty() {
      return wasm.isDirty();
    },
    onDirtyChange(callback) {
      return subscribe("dirty", callback);
    },
    onTitleChange(callback) {
      return subscribe("title", callback);
    },
    destroy() {
      wasm.destroy();
      for (const event of Object.keys(listeners)) listeners[event] = [];
      element.replaceChildren();
    },
  };
  wasm.onChange(emit("change"));
  wasm.onDirtyChange(emit("dirty"));
  wasm.onTitleChange(emit("title"));
  if (options.onChange) editor.onChange(options.onChange);
  if (options.onDirtyChange) editor.onDirtyChange(options.onDirtyChange);
  if (options.onTitleChange) editor.onTitleChange(options.onTitleChange);
  return editor;
}
