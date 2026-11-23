import { createFigureWorker } from "./figure-worker.js";

// Dependency injection keeps loader and transport checks independent of Wasm.
export function createApi({ load, wasm, createWorker, workerOptions = {} }) {
  const binary = new Set(["png", "pdf", "cdx"]);
  const figures = new Set(["svg", "png", "pdf", "cdxml", "cdx"]);
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const fonts = [];
  let loading;
  let renderer;
  const bytes = (data) => (typeof data === "string" ? encoder.encode(data) : data);
  const figure = (operation, args) => {
    renderer ??= createFigureWorker({
      module: () => wasm.workerModule(),
      direct: (name, values) => wasm[name](...values),
      fonts,
      createWorker,
      ...workerOptions,
    });
    return renderer.run(operation, args);
  };
  const api = {
    init(module) {
      loading ??= load(module === undefined ? undefined : { module_or_path: module }).catch(
        (error) => {
          loading = undefined;
          throw error;
        },
      );
      return loading.then(() => undefined);
    },
    async read(data, format) {
      await api.init();
      const result = await wasm.read(bytes(data), format);
      const output = { drawing: result.drawing, warnings: [...result.warnings] };
      result.free();
      return output;
    },
    async convert(data, from, to, options = {}) {
      await api.init();
      const args = [bytes(data), from, to, options.svgText];
      const output = await (figures.has(to) ? figure("convert", args) : wasm.convert(...args));
      return binary.has(to) ? output : decoder.decode(output);
    },
    async renderSvg(data, format, options = {}) {
      await api.init();
      return figure("renderSvg", [bytes(data), format, options.svgText]);
    },
    async renderPng(data, format, options = {}) {
      await api.init();
      return figure("renderPng", [bytes(data), format, options.maxWidth, options.maxHeight]);
    },
    async smiles(data, format) {
      await api.init();
      return wasm.smiles(bytes(data), format);
    },
    async inchi(data, format) {
      await api.init();
      return wasm.inchi(bytes(data), format);
    },
    async predictNmr(data, format, nucleus) {
      await api.init();
      return JSON.parse(await wasm.predictNmr(bytes(data), format, nucleus));
    },
    async addFont(font) {
      await api.init();
      if (typeof font === "string" || font instanceof URL) {
        const response = await fetch(font).catch((error) => {
          throw new Error(`Could not load the font ${font}: ${error?.message ?? error}`);
        });
        if (!response.ok)
          throw new Error(`Could not load the font ${font}: HTTP ${response.status}`);
        font = await response.arrayBuffer();
      }
      const input = font instanceof ArrayBuffer ? new Uint8Array(font) : font;
      const count = wasm.addFont(input);
      fonts.push(new Uint8Array(input));
      return count;
    },
    async version() {
      await api.init();
      return wasm.version();
    },
  };
  return api;
}
