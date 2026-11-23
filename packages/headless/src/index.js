// Public entry point. Static worker URL keeps bundlers aware of its module.
import load, * as wasm from "../dist/wasm/reshiki_web.js";
import { createApi } from "./api.js";

export const {
  init,
  read,
  convert,
  renderSvg,
  renderPng,
  smiles,
  inchi,
  predictNmr,
  addFont,
  version,
} = createApi({
  load,
  wasm,
  createWorker: () => new Worker(new URL("./worker.js", import.meta.url), { type: "module" }),
});
