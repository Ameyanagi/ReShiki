// The public API: a thin layer over the wasm-bindgen exports that loads the
// module once, takes text or bytes, and returns text for text formats.
import load, * as wasm from "../dist/wasm/reshiki_web.js";

const BINARY = new Set(["png", "pdf", "cdx"]);
const encoder = new TextEncoder();
const decoder = new TextDecoder();
let loading;

export function init(module) {
  loading ??= load(module === undefined ? undefined : { module_or_path: module });
  return loading.then(() => undefined);
}

const bytes = (data) => (typeof data === "string" ? encoder.encode(data) : data);

export async function convert(data, from, to) {
  await init();
  const output = await wasm.convert(bytes(data), from, to);
  return BINARY.has(to) ? output : decoder.decode(output);
}

export async function renderSvg(data, format) {
  await init();
  return wasm.renderSvg(bytes(data), format);
}

export async function renderPng(data, format, options = {}) {
  await init();
  return wasm.renderPng(bytes(data), format, options.maxWidth, options.maxHeight);
}

export async function smiles(data, format) {
  await init();
  return wasm.smiles(bytes(data), format);
}

export async function inchi(data, format) {
  await init();
  return wasm.inchi(bytes(data), format);
}

export async function version() {
  await init();
  return wasm.version();
}
